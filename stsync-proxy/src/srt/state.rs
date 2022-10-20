use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::sink::Feed;
use futures::{Sink, SinkExt};
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::sync::mpsc;

use crate::sink::{MultiSink, SessionId};

use super::conn::AckQueue;
use super::sink::OutputSink;
use super::HandshakeType;

#[derive(Clone)]
pub struct State<S>
where
    S: MultiSink,
{
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
    pub fn new(sink: S) -> Self {
        Self {
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
    inner: Mutex<HashSet<Connection<M::Sink>>>,
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
        inner.remove(&conn.server_socket_id);
        inner.insert(conn);
    }

    pub fn remove<T>(&self, conn: T)
    where
        T: Borrow<SocketId>,
    {
        self.inner.lock().unwrap().remove(conn.borrow());
    }

    pub fn get<T>(&self, conn: T) -> Option<Connection<M::Sink>>
    where
        T: Borrow<SocketId>,
    {
        self.inner.lock().unwrap().get(conn.borrow()).cloned()
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

    pub inflight_acks: AckQueue,

    pub sink: Arc<Mutex<Option<OutputSink<S>>>>,
}

impl<S> Connection<S>
where
    S: Sink<Vec<u8>>,
{
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
            inflight_acks: AckQueue::new(),
            sink: Arc::default(),
        }
    }

    pub fn timestamp(&self) -> u32 {
        Instant::now().duration_since(self.start_time).as_micros() as u32
    }

    pub fn update_rtt(&mut self, rtt: u32) {
        // See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.10
        // Update RTT variance before RTT because the current RTT is required
        // to calculate the RTT variance.
        self.rtt_variance = (3 / 4) * self.rtt_variance + (1 / 4) * self.rtt.abs_diff(rtt);
        self.rtt = (7 / 8) * self.rtt + (1 / 8) * rtt;
    }

    pub async fn write_sink<M>(
        &self,
        state: &State<M>,
        seqnum: u32,
        buf: Vec<u8>,
    ) -> Result<(), S::Error>
    where
        M: MultiSink<Sink = S>,
        S: Unpin,
    {
        let mut sink = self.sink.lock().unwrap();

        match &mut *sink {
            Some(sink) => sink.push(seqnum, buf).await,
            None => {
                let new = state.sink.attach(SessionId(self.server_socket_id.0 as u64));
                *sink = Some(OutputSink::new(new, seqnum));
                sink.as_mut().unwrap().push(seqnum, buf).await
            }
        }
    }
}

impl<S> Clone for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            server_socket_id: self.server_socket_id,
            client_socket_id: self.client_socket_id,
            server_sequence_number: self.server_sequence_number,
            client_sequence_number: self.client_sequence_number,
            rtt: self.rtt,
            rtt_variance: self.rtt_variance,
            state: self.state,
            syn_cookie: self.syn_cookie,
            inflight_acks: self.inflight_acks.clone(),
            sink: self.sink.clone(),
        }
    }
}

impl<S> Borrow<SocketId> for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn borrow(&self) -> &SocketId {
        &self.server_socket_id
    }
}

impl<S> PartialEq for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn eq(&self, other: &Self) -> bool {
        self.server_socket_id == other.server_socket_id
    }
}

impl<S> Hash for Connection<S>
where
    S: Sink<Vec<u8>>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.server_socket_id.hash(state);
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
