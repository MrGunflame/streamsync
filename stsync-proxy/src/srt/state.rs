use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::SinkExt;
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::sync::{mpsc, Notify};

use crate::proto::Encode;
use crate::session::{ResourceId, SessionManager};

use super::config::Config;
use super::conn::AckQueue;
use super::metrics::ConnectionMetrics;
use super::sink::OutputSink;
use super::{DataPacket, Header, IsPacket};

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
}

impl<S> StateInner<S>
where
    S: SessionManager,
{
    pub fn random(&self) -> u32 {
        self.prng.lock().unwrap().next_u32()
    }
}

#[derive(Debug)]
pub struct ConnectionPool {
    inner: Mutex<HashSet<Arc<Connection>>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            inner: Mutex::default(),
        }
    }

    pub fn insert(&self, conn: Connection) {
        let mut inner = self.inner.lock().unwrap();
        inner.insert(Arc::new(conn));
    }

    pub fn remove<T>(&self, conn: T)
    where
        T: Borrow<ConnectionId>,
    {
        let mut inner = self.inner.lock().unwrap();
        inner.remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<Arc<Connection>>
    where
        T: Borrow<ConnectionId>,
    {
        self.inner.lock().unwrap().get(conn.borrow()).cloned()
    }

    pub async fn clean(&self) -> usize {
        let removed = {
            let mut inner = self.inner.lock().unwrap();

            let mut removed = Vec::new();
            inner.retain(|conn| {
                if conn.last_packet_time.lock().unwrap().elapsed() > Duration::from_secs(15) {
                    removed.push(conn.clone());
                    false
                } else {
                    true
                }
            });

            removed
        };

        let num_removed = removed.len();
        for conn in removed.into_iter() {
            conn.close();
        }

        num_removed
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn find_client_id(&self, addr: SocketAddr, socket_id: u32) -> Option<Arc<Connection>> {
        let inner = self.inner.lock().unwrap();

        for conn in &*inner {
            if conn.id.addr == addr && conn.client_socket_id == socket_id {
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
}

impl Connection {
    pub fn new<T, U>(addr: SocketAddr, server_socket_id: T, client_socket_id: U) -> Self
    where
        T: Into<SocketId>,
        U: Into<SocketId>,
    {
        let server_socket_id = server_socket_id.into();

        // This is about a 1.5MB backlog with an MTU of 1500.
        let (tx, rx) = mpsc::channel(1024);

        Self {
            id: ConnectionId {
                addr,
                socket_id: server_socket_id,
            },
            start_time: Instant::now(),
            server_socket_id: server_socket_id.into(),
            client_socket_id: client_socket_id.into(),
            client_sequence_number: AtomicU32::new(0),
            server_sequence_number: AtomicU32::new(0),
            rtt: Rtt::new(),
            state: ConnectionState::new(),
            syn_cookie: AtomicU32::new(0),
            inflight_acks: Mutex::new(AckQueue::new()),
            mode: Mutex::default(),
            last_packet_time: Mutex::new(Instant::now()),
            packets_recv: AtomicU32::new(0),
            bytes_recv: AtomicU32::new(0),
            metrics: ConnectionMetrics::new(),
            lost_packets: Mutex::default(),
            shutdown: Notify::new(),
            tx,
            rx: Mutex::new(Some(rx)),
        }
    }

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

    pub fn spawn_publish<S>(self: &Arc<Self>, state: &State<S>, resource_id: Option<ResourceId>)
    where
        S: SessionManager,
    {
        let mut rx = self.rx.lock().unwrap().take().unwrap();

        let sink = state.session_manager.publish(resource_id).unwrap();

        let mut sink = OutputSink::<S>::new(Arc::downgrade(self), sink);

        tokio::task::spawn(async move {
            while let Some(packet) = rx.recv().await {
                let _ = sink.push(packet).await;
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
    pub fn new() -> Self {
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
            .fetch_update(Ordering::Release, Ordering::Acquire, |bits| {
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionId {
    pub addr: SocketAddr,
    pub socket_id: SocketId,
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
