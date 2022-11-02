use std::borrow::Borrow;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::AHashMap;
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
