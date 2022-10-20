use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct State {
    pub connections: Arc<Mutex<HashSet<Connection>>>,
}

#[derive(Debug)]
pub struct ConnectionPool {
    inner: HashSet<Connection>,
}

impl ConnectionPool {
    pub fn insert(&mut self, conn: Connection) {
        self.inner.insert(conn);
    }

    pub fn remove<T>(&mut self, conn: T)
    where
        T: Borrow<SocketId>,
    {
        self.inner.remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<Connection>
    where
        T: Borrow<SocketId>,
    {
        self.inner.get(conn.borrow()).copied()
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
        }
    }

    pub fn timestamp(&self) -> u32 {
        Instant::now().duration_since(self.start_time).as_millis() as u32
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
