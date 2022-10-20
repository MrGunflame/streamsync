use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rand::rngs::OsRng;
use rand::RngCore;

use super::HandshakeType;

#[derive(Clone, Debug)]
pub struct State {
    pub pool: Arc<ConnectionPool>,
    /// Pseudo RNG for all non-crypto randomness
    // NOTE: This actually is a CSPRNG but it doesn't have to be.
    pub prng: Arc<Mutex<OsRng>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(ConnectionPool::default()),
            prng: Arc::default(),
        }
    }

    pub fn random(&self) -> u32 {
        self.prng.lock().unwrap().next_u32()
    }
}

#[derive(Debug, Default)]
pub struct ConnectionPool {
    inner: Mutex<HashSet<Connection>>,
}

impl ConnectionPool {
    pub fn insert(&self, conn: Connection) {
        self.inner.lock().unwrap().insert(conn);
    }

    pub fn remove<T>(&self, conn: T)
    where
        T: Borrow<SocketId>,
    {
        self.inner.lock().unwrap().remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<Connection>
    where
        T: Borrow<SocketId>,
    {
        self.inner.lock().unwrap().get(conn.borrow()).copied()
    }
}

/// All SRT traffic flows over a single UDP socket. A [`Connection`] represents a single stream
/// to a peer multiplexed over that UDP socket.
///
/// On the initial handshake the client and server will exchange their [`SocketId`]s. When
/// receiving a packet it must be matched against the `server_socket_id` to find an already
/// existing stream. When sending a packet it must be sent to the `client_socket_id`.
#[derive(Copy, Clone, Debug)]
pub struct Connection {
    /// Starting time of the connection. Used to calculate packet timestamps.
    pub start_time: Instant,
    pub server_socket_id: SocketId,
    /// Socket id of the the remote peer.
    pub client_socket_id: SocketId,
    pub client_sequence_number: u32,
    pub server_sequence_number: u32,
    pub rtt: u32,
    pub rtt_variance: u32,
    pub state: HandshakeType,
    pub syn_cookie: u32,
}

impl Connection {
    pub fn new<T, U>(server_socket_id: T, client_socket_id: U) -> Self
    where
        T: Into<SocketId>,
        U: Into<SocketId>,
    {
        Self {
            start_time: Instant::now(),
            server_socket_id: server_socket_id.into(),
            client_socket_id: client_socket_id.into(),
            client_sequence_number: 0,
            server_sequence_number: 0,
            // Default 100ms
            rtt: 100_000,
            // Default 50ms
            rtt_variance: 50_000,
            state: HandshakeType::Induction,
            syn_cookie: 0,
        }
    }

    pub fn timestamp(&self) -> u32 {
        Instant::now().duration_since(self.start_time).as_millis() as u32
    }

    pub fn update_rtt(&mut self, rtt: u32) {
        // See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.10
        // Update RTT variance before RTT because the current RTT is required
        // to calculate the RTT variance.
        self.rtt_variance = (3 / 4) * self.rtt_variance + (1 / 4) * self.rtt.abs_diff(rtt);
        self.rtt = (7 / 8) * self.rtt + (1 / 8) * rtt;
    }
}

impl Borrow<SocketId> for Connection {
    fn borrow(&self) -> &SocketId {
        &self.server_socket_id
    }
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.server_socket_id == other.server_socket_id
    }
}

impl Hash for Connection {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.server_socket_id.hash(state);
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
