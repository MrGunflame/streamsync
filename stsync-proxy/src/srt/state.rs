use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::num::Wrapping;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::sink::Feed;
use futures::{Sink, SinkExt};
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::sync::mpsc;

use crate::sink::{MultiSink, SessionId};

use super::config::Config;
use super::conn::AckQueue;
use super::metrics::ConnectionMetrics;
use super::sink::OutputSink;
use super::{DataPacket, HandshakeType, Header};

#[derive(Clone)]
pub struct State<S>
where
    S: MultiSink,
{
    pub config: Arc<Config>,
    pub pool: Arc<ConnectionPool<S>>,
    /// Pseudo RNG for all non-crypto randomness
    // NOTE: This actually is a CSPRNG but it doesn't have to be.
    pub prng: Arc<Mutex<OsRng>>,
    pub sink: S,
}

impl<S> State<S>
where
    S: MultiSink,
{
    pub fn new(sink: S, config: Config) -> Self {
        Self {
            config: Arc::new(config),
            pool: Arc::new(ConnectionPool::new()),
            prng: Arc::default(),
            sink,
        }
    }

    pub fn random(&self) -> u32 {
        self.prng.lock().unwrap().next_u32()
    }
}

#[derive(Debug)]
pub struct ConnectionPool<M>
where
    M: MultiSink,
{
    inner: Mutex<HashSet<Arc<Connection<M::Sink>>>>,
}

impl<M> ConnectionPool<M>
where
    M: MultiSink,
{
    pub fn new() -> Self {
        Self {
            inner: Mutex::default(),
        }
    }

    pub fn insert(&self, conn: Connection<M::Sink>) {
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

    pub fn get<T>(&self, conn: T) -> Option<Arc<Connection<M::Sink>>>
    where
        T: Borrow<ConnectionId>,
    {
        self.inner.lock().unwrap().get(conn.borrow()).cloned()
    }

    pub fn clean(&self) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let mut num_removed = 0;
        inner.retain(|conn| {
            if conn.last_packet_time.lock().unwrap().elapsed() > Duration::from_secs(15) {
                num_removed += 1;
                false
            } else {
                true
            }
        });

        num_removed
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn find_client_id(
        &self,
        addr: SocketAddr,
        socket_id: u32,
    ) -> Option<Arc<Connection<M::Sink>>> {
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
pub struct Connection<S>
where
    S: Sink<Vec<u8>>,
{
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

    /// Total packets/bytes received from the peer.
    // TODO: Merge together into single cell
    pub packets_recv: AtomicU32,
    pub bytes_recv: AtomicU32,

    pub sink: Mutex<Option<OutputSink<S>>>,
    /// Relative timestamp of the last transmitted message. We drop the connection when this
    /// reaches 5s.
    pub last_packet_time: Mutex<Instant>,
    pub metrics: ConnectionMetrics,
}

impl<S> Connection<S>
where
    S: Sink<Vec<u8>> + Unpin,
{
    pub fn new<T, U>(addr: SocketAddr, server_socket_id: T, client_socket_id: U) -> Self
    where
        T: Into<SocketId>,
        U: Into<SocketId>,
    {
        let server_socket_id = server_socket_id.into();

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
            sink: Mutex::default(),
            last_packet_time: Mutex::new(Instant::now()),
            packets_recv: AtomicU32::new(0),
            bytes_recv: AtomicU32::new(0),
            metrics: ConnectionMetrics::new(),
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

    pub async fn write_sink<M>(&self, state: &State<M>, packet: DataPacket) -> Result<(), S::Error>
    where
        M: MultiSink<Sink = S>,
    {
        let mut sink = self.sink.lock().unwrap();

        let seqnum = packet.packet_sequence_number();

        match &mut *sink {
            Some(sink) => sink.push(packet).await,
            None => {
                let new = state.sink.attach(SessionId(self.server_socket_id.0 as u64));
                *sink = Some(OutputSink::new(self, new, seqnum));
                sink.as_mut().unwrap().push(packet).await
            }
        }
    }

    pub async fn close(&self) {
        let mut sink = self.sink.lock().unwrap();
        if let Some(sink) = &mut *sink {
            let _ = sink.close().await;
        }
    }
}

impl<'a, S> Borrow<ConnectionId> for Arc<Connection<S>>
where
    S: Sink<Vec<u8>>,
{
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl<S> Borrow<ConnectionId> for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl<S> PartialEq<ConnectionId> for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn eq(&self, other: &ConnectionId) -> bool {
        self.id == *other
    }
}

impl<S> PartialEq for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<S> Hash for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<S> Eq for Connection<S> where S: Sink<Vec<u8>> {}

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

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, net::SocketAddr};

    use crate::{
        sink::NullSink,
        srt::state::{Connection, ConnectionId, SocketId},
    };

    use super::Rtt;

    #[test]
    fn test_set() {
        let addr = SocketAddr::from(([127, 0, 0, 1], 80));
        let socket_id = SocketId(50);
        let mut set = HashSet::<Connection<NullSink>>::new();
        set.insert(Connection::new(addr, socket_id, 0));

        assert!(set.get(&ConnectionId { addr, socket_id }).is_some());
    }

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
