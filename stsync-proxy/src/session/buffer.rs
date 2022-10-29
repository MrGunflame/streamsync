use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{Sink, Stream, StreamExt};
use snowflaked::sync::Generator;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::{Error, LiveSink, LiveStream, ResourceId, SessionManager};

#[derive(Debug)]
pub struct BufferSessionManager {
    resource_id: Arc<Generator>,
    streams: Arc<Mutex<HashMap<ResourceId, broadcast::Sender<Bytes>>>>,
}

impl BufferSessionManager {
    pub fn new() -> Self {
        Self {
            resource_id: Arc::new(Generator::new(0)),
            streams: Arc::default(),
        }
    }
}

impl SessionManager for BufferSessionManager {
    type Sink = BufferSink;
    type Stream = BufferStream;

    fn request(&self, resource_id: ResourceId) -> Result<LiveStream<Self::Stream>, Error> {
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

    fn publish(&self, resource_id: Option<ResourceId>) -> Result<LiveSink<Self::Sink>, Error> {
        let mut streams = self.streams.lock().unwrap();

        let id = match resource_id {
            Some(id) => id,
            None => self.resource_id.generate(),
        };

        let tx = match streams.get(&id) {
            // Attach to existing stream.
            Some(tx) => tx.clone(),
            None => {
                let (tx, _) = broadcast::channel(1024);
                streams.insert(id, tx.clone());
                tx
            }
        };

        Ok(LiveSink::new(id, BufferSink { tx }))
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
