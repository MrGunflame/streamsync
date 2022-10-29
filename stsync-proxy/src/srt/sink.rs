//! Output sinks
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Weak;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{Sink, SinkExt};

use crate::session::{LiveSink, SessionManager};

use super::state::Connection;
use super::DataPacket;

// TODO: OutputSink should implement futures::Sink instead of a "push" method.
#[derive(Debug)]
pub struct OutputSink<S>
where
    S: SessionManager,
{
    /// Sequence number of the next expected segment.
    next_msgnum: u32,
    sink: LiveSink<S::Sink>,
    queue: BufferQueue,
    skip_counter: u8,

    conn: Weak<Connection>,
}

impl<S> OutputSink<S>
where
    S: SessionManager,
{
    pub fn new(conn: Weak<Connection>, sink: LiveSink<S::Sink>) -> Self {
        Self {
            next_msgnum: 1,
            sink,
            queue: BufferQueue::new(),
            skip_counter: 0,
            conn,
        }
    }

    pub async fn push(
        &mut self,
        packet: DataPacket,
    ) -> Result<(), <S::Sink as Sink<Bytes>>::Error> {
        let msgnum = packet.message_number();

        // We already assumed the packets to be lost and skipped ahead.
        if self.next_msgnum > msgnum {
            tracing::trace!("Segment {} received too late", msgnum);
            return Ok(());
        }

        if self.next_msgnum == msgnum {
            tracing::trace!("Segment {} is in order", msgnum);
            self.skip_counter = self.skip_counter.saturating_sub(1);

            self.sink.feed(packet.data.into()).await?;
            self.next_msgnum += 1;

            loop {
                match self.queue.remove(self.next_msgnum) {
                    Some(buf) => {
                        self.sink.feed(buf.into()).await?;
                        self.next_msgnum += 1;
                    }
                    None => break,
                }
            }
        } else {
            self.skip_counter += 1;

            tracing::debug!(
                "Segment {} is out of order (missing {}) (skip ahead in {})",
                msgnum,
                self.next_msgnum,
                self.skip_counter,
            );

            self.queue.insert(msgnum, packet.data);

            if self.skip_counter >= 20 {
                tracing::trace!("Skip ahead (20 segments)");
                while self.next_msgnum <= msgnum {
                    if let Some(buf) = self.queue.remove(self.next_msgnum) {
                        self.sink.feed(buf.into()).await?;
                        self.skip_counter -= 1;
                    }

                    self.next_msgnum += 1;
                }

                tracing::trace!("Lost {} segments", self.skip_counter);

                if let Some(conn) = self.conn.upgrade() {
                    conn.metrics.packets_dropped.add(self.skip_counter as usize);
                }

                self.skip_counter = 0;
            }
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), <S::Sink as Sink<Bytes>>::Error> {
        tracing::debug!("Dropping {} bytes from queue", self.queue.size);
        self.queue.clear();

        self.sink.close().await
    }

    fn sink(self: Pin<&mut Self>) -> Pin<&mut LiveSink<S::Sink>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }
}

unsafe impl<S> Send for OutputSink<S> where S: SessionManager + Send {}
unsafe impl<S> Sync for OutputSink<S> where S: SessionManager + Sync {}

#[derive(Clone, Debug, Default)]
pub struct BufferQueue {
    inner: HashMap<u32, Vec<u8>>,
    /// Total size of all buffers combined.
    size: usize,
}

impl BufferQueue {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            size: 0,
        }
    }

    pub fn insert(&mut self, seq: u32, buf: Vec<u8>) {
        // We never store empty buffers.
        if buf.len() != 0 {
            self.size += buf.len();
            self.inner.insert(seq, buf);
        }
    }

    pub fn remove(&mut self, seq: u32) -> Option<Vec<u8>> {
        match self.inner.remove(&seq) {
            Some(buf) => {
                self.size -= buf.len();
                Some(buf)
            }
            None => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.size = 0;
    }
}
