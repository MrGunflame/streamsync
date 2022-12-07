use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use futures::{Sink, Stream, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use snowflaked::sync::Generator;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::{Error, LiveSink, LiveStream, ResourceId, SessionId, SessionManager};

#[derive(Clone, Debug)]
pub struct BufferSessionManager(Arc<Inner>);

impl Deref for BufferSessionManager {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct Inner {
    resource_id: Generator,
    streams: Mutex<HashMap<ResourceId, broadcast::Sender<Bytes>>>,
    pub registry: SessionRegistry,
}

impl BufferSessionManager {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            resource_id: Generator::new(0),
            streams: Default::default(),
            registry: SessionRegistry::new(),
        }))
    }
}

impl SessionManager for BufferSessionManager {
    type Sink = BufferSink;
    type Stream = BufferStream;

    fn request(
        &self,
        resource_id: Option<ResourceId>,
        session_id: Option<SessionId>,
    ) -> Result<LiveStream<Self::Stream>, Error> {
        let resource_id = resource_id.ok_or(Error::InvalidResourceId)?;
        let session_id = session_id.ok_or(Error::InvalidCredentials)?;

        match self.registry.remove(resource_id, session_id) {
            Some(key) => {
                if key.session_id != session_id || key.is_expired() {
                    tracing::debug!("Rejecting due to invalid or expired key");
                    return Err(Error::InvalidCredentials);
                }
            }
            None => return Err(Error::InvalidCredentials),
        }

        let mut streams = self.streams.lock().unwrap();

        let rx = match streams.get(&resource_id) {
            Some(rx) => rx.subscribe(),
            None => {
                let (tx, rx) = broadcast::channel(1024);

                streams.insert(resource_id, tx);
                rx
            }
        };

        let stream = BufferStream {
            stream: BroadcastStream::new(rx.resubscribe()),
        };

        Ok(LiveStream::new(resource_id, stream))
    }

    fn publish(
        &self,
        resource_id: Option<ResourceId>,
        session_id: Option<SessionId>,
    ) -> Result<LiveSink<Self::Sink>, Error> {
        let resource_id = resource_id.ok_or(Error::InvalidResourceId)?;
        let session_id = session_id.ok_or(Error::InvalidCredentials)?;

        match self.registry.remove(resource_id, session_id) {
            Some(key) => {
                if key.session_id != session_id || key.is_expired() {
                    tracing::debug!("Rejecting due to invalid or expired key");
                    return Err(Error::InvalidCredentials);
                }
            }
            None => return Err(Error::InvalidCredentials),
        }

        let mut streams = self.streams.lock().unwrap();

        let tx = match streams.get(&resource_id) {
            // Attach to existing stream.
            Some(tx) => tx.clone(),
            None => {
                let (tx, _) = broadcast::channel(1024);
                streams.insert(resource_id, tx.clone());
                tx
            }
        };

        Ok(LiveSink::new(resource_id, BufferSink { tx }))
    }
}

#[derive(Debug)]
pub struct BufferSink {
    tx: broadcast::Sender<Bytes>,
}

impl Sink<Bytes> for BufferSink {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let _ = self.tx.send(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
pub struct BufferStream {
    stream: BroadcastStream<Bytes>,
}

impl Stream for BufferStream {
    type Item = Bytes;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(bytes)),
            Poll::Ready(Some(Err(_))) => Poll::Ready(None),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    /// ResourceId => SessionId, Expires
    inner: RwLock<HashMap<ResourceId, Vec<SessionKey>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, key: SessionKey) {
        let mut inner = self.inner.write();
        match inner.get_mut(&key.resource_id) {
            Some(vec) => vec.push(key),
            None => {
                inner.insert(key.resource_id, vec![key]);
            }
        }
    }

    pub fn get(&self, resource_id: ResourceId, session_id: SessionId) -> Option<SessionKey> {
        let inner = self.inner.read();
        for key in inner.get(&resource_id)? {
            if key.session_id == session_id {
                return Some(*key);
            }
        }

        None
    }

    pub fn remove(&self, resource_id: ResourceId, session_id: SessionId) -> Option<SessionKey> {
        let mut inner = self.inner.write();
        let keys = inner.get_mut(&resource_id)?;

        let mut index = 0;
        while index < keys.len() {
            let key = &keys[index];

            if key.session_id == session_id {
                return Some(keys.remove(index));
            }

            index += 1;
        }

        None
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SessionKey {
    pub resource_id: ResourceId,
    pub session_id: SessionId,
    pub expires: Instant,
}

impl SessionKey {
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires
    }
}

impl Hash for SessionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.resource_id.hash(state);
    }
}

impl PartialEq for SessionKey {
    fn eq(&self, other: &Self) -> bool {
        self.resource_id == other.resource_id
    }
}

impl PartialEq<ResourceId> for SessionKey {
    fn eq(&self, other: &ResourceId) -> bool {
        self.resource_id == *other
    }
}

impl Borrow<ResourceId> for SessionKey {
    fn borrow(&self) -> &ResourceId {
        &self.resource_id
    }
}

impl Eq for SessionKey {}
