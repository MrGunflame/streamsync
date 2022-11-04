use std::borrow::Borrow;
use std::fmt::Display;
use std::hash::Hash;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use parking_lot::{Mutex, RwLock};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::session::SessionManager;

use super::config::Config;
use super::conn::ConnectionHandle;
use super::metrics::{ConnectionMetrics, ServerMetrics};

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
                conn_metrics: Mutex::new(AHashMap::new()),
                metrics: ServerMetrics::new(),
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
    pub conn_metrics: Mutex<AHashMap<ConnectionId, Arc<ConnectionMetrics>>>,
    pub metrics: ServerMetrics,
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
    inner: RwLock<AHashSet<ConnectionHandle>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            inner: RwLock::default(),
        }
    }

    pub fn insert(&self, handle: ConnectionHandle) {
        let mut inner = self.inner.write();
        inner.insert(handle);
    }

    pub fn remove<T>(&self, conn: T)
    where
        T: Borrow<ConnectionId>,
    {
        let mut inner = self.inner.write();
        inner.remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<ConnectionHandle>
    where
        T: Borrow<ConnectionId>,
    {
        self.inner.read().get(conn.borrow()).cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn find_client_id(&self, addr: SocketAddr, socket_id: u32) -> Option<ConnectionHandle> {
        let inner = self.inner.read();

        for handle in &*inner {
            if handle.id.addr == addr && handle.id.client_socket_id == socket_id {
                return Some(handle.clone());
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
