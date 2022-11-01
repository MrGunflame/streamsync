use std::borrow::Borrow;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use futures::StreamExt;
use parking_lot::{Mutex, RwLock};
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Notify};

use crate::proto::Encode;
use crate::session::{ResourceId, SessionManager};
use crate::srt::PacketType;

use super::config::Config;
use super::conn::AckQueue;
use super::metrics::ConnectionMetrics;
use super::sink::OutputSink;
use super::{DataPacket, Header, IsPacket, Packet};

#[derive(Debug)]
pub struct State<S>
where
    S: SessionManager,
{
    inner: Arc<StateInner<S>>,
}

impl<S> State<S>
where
    S: SessionManager,
{
    pub fn new(session_manager: S, config: Config) -> Self {
        Self {
            inner: Arc::new(StateInner {
                config: config,
                pool: ConnectionPool::new(),
                prng: Mutex::new(OsRng),
                session_manager,
                metrics: Mutex::new(AHashMap::new()),
            }),
        }
    }
}

impl<S> Clone for State<S>
where
    S: SessionManager,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> Deref for State<S>
where
    S: SessionManager,
{
    type Target = StateInner<S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug)]
pub struct StateInner<S>
where
    S: SessionManager,
{
    pub config: Config,
    pub pool: ConnectionPool,
    /// Pseudo RNG for all non-crypto randomness
    // NOTE: This actually is a CSPRNG but it doesn't have to be.
    pub prng: Mutex<OsRng>,
    pub session_manager: S,
    pub metrics: Mutex<AHashMap<ConnectionId, Arc<ConnectionMetrics>>>,
}

impl<S> StateInner<S>
where
    S: SessionManager,
{
    pub fn random(&self) -> u32 {
        self.prng.lock().next_u32() >> 1
    }
}

#[derive(Debug)]
pub struct ConnectionPool {
    inner: RwLock<AHashMap<ConnectionId, mpsc::Sender<Packet>>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            inner: RwLock::default(),
        }
    }

    pub fn insert(&self, id: ConnectionId, tx: mpsc::Sender<Packet>) {
        let mut inner = self.inner.write();
        inner.insert(id, tx);
    }

    pub fn remove<T>(&self, conn: T)
    where
        T: Borrow<ConnectionId>,
    {
        let mut inner = self.inner.write();
        inner.remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<mpsc::Sender<Packet>>
    where
        T: Borrow<ConnectionId>,
    {
        self.inner.read().get(conn.borrow()).cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn find_client_id(&self, addr: SocketAddr, socket_id: u32) -> Option<mpsc::Sender<Packet>> {
        let inner = self.inner.read();

        for (id, conn) in &*inner {
            if id.addr == addr && id.client_socket_id == socket_id {
                return Some(conn.clone());
            }
        }

        None
    }
}

/// All SRT traffic flows over a single UDP socket. A [`Connection`] represents a single stream
/// to a peer multiplexed over that UDP socket.
///
/// On the initial handshake the client and server will exchange their [`SocketId`]s. When
/// receiving a packet it must be matched against the `server_socket_id` to find an already
/// existing stream. When sending a packet it must be sent to the `client_socket_id`.
#[derive(Debug)]
pub struct Connection {
    pub id: ConnectionId,

    /// Starting time of the connection. Used to calculate packet timestamps.
    pub start_time: Instant,
    pub server_socket_id: SocketId,
    /// Socket id of the the remote peer.
    pub client_socket_id: SocketId,
    pub client_sequence_number: AtomicU32,
    pub server_sequence_number: AtomicU32,

    pub rtt: Rtt,
    pub state: ConnectionState,
    // TODO: Merge with state.
    pub syn_cookie: AtomicU32,

    pub inflight_acks: Mutex<AckQueue>,
    pub lost_packets: Mutex<Vec<u32>>,

    /// Total packets/bytes received from the peer.
    // TODO: Merge together into single cell
    pub packets_recv: AtomicU32,
    pub bytes_recv: AtomicU32,

    pub mode: Mutex<ConnectionMode>,
    /// Relative timestamp of the last transmitted message. We drop the connection when this
    /// reaches 5s.
    pub last_packet_time: Mutex<Instant>,
    pub metrics: ConnectionMetrics,

    pub shutdown: Notify,

    pub tx: mpsc::Sender<DataPacket>,
    pub rx: Mutex<Option<mpsc::Receiver<DataPacket>>>,

    /// The number of free buffers avaliable. Defaults to 8192.
    pub buffers_avail: AtomicU32,
    /// Async waker that should be called if a buffer comes avaliable after it reached 0.
    pub buffers_waker: Notify,
}

impl Connection {
    pub fn timestamp(&self) -> u32 {
        Instant::now().duration_since(self.start_time).as_micros() as u32
    }

    pub fn delivery_time<T>(&self, header: T) -> Instant
    where
        T: AsRef<Header>,
    {
        let time = Duration::from_micros((header.as_ref().timestamp + self.rtt.load().0) as u64);
        self.start_time + time
    }

    pub async fn send(&self, packet: DataPacket) {
        let _ = self.tx.send(packet).await;
    }

    pub fn close(&self) {
        self.shutdown.notify_one();
    }

    pub fn spawn_request<S>(
        self: &Arc<Self>,
        socket: Arc<UdpSocket>,
        state: &State<S>,
        resource_id: ResourceId,
    ) where
        S: SessionManager,
    {
        let mut stream = match state.session_manager.request(resource_id) {
            Ok(s) => s,
            Err(_) => return,
        };

        let conn = self.clone();

        tokio::task::spawn(async move {
            let mut msgnum = 1;

            let shutdown = conn.shutdown.notified();

            let fut = async {
                while let Some(buf) = stream.next().await {
                    // Wait for more buffers to become available.
                    // if conn
                    //     .buffers_avail
                    //     .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    //         if n == 0 {
                    //             None
                    //         } else {
                    //             Some(n - 1)
                    //         }
                    //     })
                    //     .is_err()
                    // {
                    //     tracing::info!("Buffering");
                    //     conn.buffers_waker.notified().await;
                    // }

                    let seqnum = conn.server_sequence_number.fetch_add(1, Ordering::AcqRel);

                    let mut packet = DataPacket::default();
                    packet.header.set_packet_type(PacketType::Data);
                    packet.header.timestamp = conn.timestamp();
                    packet.header.destination_socket_id = conn.client_socket_id.0;
                    packet.header().set_packet_sequence_number(seqnum);
                    packet.header().set_message_number(msgnum);
                    packet.header().set_ordered(true);
                    packet
                        .header()
                        .set_packet_position(crate::srt::PacketPosition::Full);
                    packet.data = buf.into();

                    let addr = conn.id.addr;
                    socket
                        .send_to(&packet.upcast().encode_to_vec().unwrap(), addr)
                        .await
                        .unwrap();

                    msgnum += 1;
                }
            };

            tokio::select! {
                _ = fut => {}
                _ = shutdown => {}
            }
        });
    }
}

impl<'a> Borrow<ConnectionId> for Arc<Connection> {
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl Borrow<ConnectionId> for Connection {
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl PartialEq<ConnectionId> for Connection {
    fn eq(&self, other: &ConnectionId) -> bool {
        self.id == *other
    }
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for Connection {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Eq for Connection {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketId(pub u32);

impl PartialEq<u32> for SocketId {
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl From<u32> for SocketId {
    fn from(src: u32) -> Self {
        Self(src)
    }
}

#[derive(Debug)]
pub struct ConnectionState {
    inner: AtomicU8,
}

impl ConnectionState {
    pub const NULL: u8 = 0;
    pub const INDUCTION: u8 = 1;
    pub const DONE: u8 = 2;

    pub fn new() -> Self {
        Self {
            inner: AtomicU8::new(Self::NULL),
        }
    }

    pub fn set(&self, state: u8) {
        self.inner.store(state, Ordering::Release);
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ConnectionMode {
    /// Connection mode is currently undefined. This state can only be present while a handshake
    /// is currently being made.
    #[default]
    Undefined,
    /// The caller (client) wants to receive data.
    Request,
    /// The caller (client) wants to send data.
    Publish,
}

/// A single atomic cell containing both RTT and RTT variance.
#[derive(Debug)]
#[repr(transparent)]
pub struct Rtt {
    // 1. 32-bit RTT
    // 2. 32-bit RTT variance
    cell: AtomicU64,
}

impl Rtt {
    /// Default RTT of 100ms and RTT variance of 50ms.
    pub const fn new() -> Self {
        let bits = Self::to_bits(100_000, 50_000);

        Self {
            cell: AtomicU64::new(bits),
        }
    }

    pub fn load(&self) -> (u32, u32) {
        let bits = self.cell.load(Ordering::Relaxed);
        Self::from_bits(bits)
    }

    pub fn update(&self, new: u32) {
        // See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.10
        let _ = self
            .cell
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                let (mut rtt, mut var) = Self::from_bits(bits);

                var = ((3.0 / 4.0) * var as f32 + (1.0 / 4.0) * rtt.abs_diff(new) as f32) as u32;
                rtt = ((7.0 / 8.0) * rtt as f32 + (1.0 / 8.0) * new as f32) as u32;

                Some(Self::to_bits(rtt, var))
            });
    }

    const fn from_bits(bits: u64) -> (u32, u32) {
        let rtt = (bits >> 32) as u32;
        let var = (bits & ((1 << 31) - 1)) as u32;
        (rtt, var)
    }

    const fn to_bits(rtt: u32, var: u32) -> u64 {
        ((rtt as u64) << 32) + var as u64
    }
}

impl Default for Rtt {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionId {
    pub addr: SocketAddr,
    pub server_socket_id: SocketId,
    pub client_socket_id: SocketId,
}

impl Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}]:{}:{}",
            self.addr, self.server_socket_id.0, self.client_socket_id.0
        )
    }
}

pub enum Signal {
    Data(DataPacket),
    Ack(),
}

#[cfg(test)]
mod tests {
    use super::Rtt;

    #[test]
    fn test_rtt() {
        let rtt = Rtt::new();
        assert_eq!(rtt.load(), (100_000, 50_000));
        rtt.update(100_000);
        assert_eq!(rtt.load(), (100_000, 37_500));

        let rtt = Rtt::new();
        rtt.update(0);
        assert_eq!(rtt.load(), (87_500, 62_500));
    }
}
