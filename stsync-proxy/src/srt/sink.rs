//! Output sinks
use std::collections::HashMap;

use futures::{Sink, SinkExt};

#[derive(Debug)]
pub struct OutputSink<S>
where
    S: Sink<Vec<u8>>,
{
    /// Sequence number of the next expected segment.
    next_seqnum: u32,
    sink: S,
    queue: BufferQueue,
}

impl<S> OutputSink<S>
where
    S: Sink<Vec<u8>> + Unpin,
{
    pub fn new(sink: S, seqnum: u32) -> Self {
        Self {
            next_seqnum: seqnum,
            sink,
            queue: BufferQueue::new(),
        }
    }

    pub async fn push(&mut self, seqnum: u32, buf: Vec<u8>) -> Result<(), S::Error> {
        // We already assumed the packets to be lost and skipped ahead.
        if self.next_seqnum > seqnum {
            return Ok(());
        }

        if self.next_seqnum == seqnum {
            tracing::trace!("Segment {} is in order", seqnum);

            self.sink.feed(buf).await?;
            self.next_seqnum += 1;

            loop {
                match self.queue.remove(self.next_seqnum) {
                    Some(buf) => {
                        self.sink.feed(buf).await?;
                        self.next_seqnum += 1;
                    }
                    None => break,
                }
            }
        } else {
            tracing::debug!(
                "Segment {} is out of order (missing {})",
                seqnum,
                self.next_seqnum
            );

            self.queue.insert(seqnum, buf);
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), S::Error> {
        tracing::debug!("Dropping {} bytes from queue", self.queue.size);
        self.queue.clear();

        self.sink.close().await
    }
}

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
        self.size += buf.len();
        self.inner.insert(seq, buf);
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

    pub fn clear(&mut self) {
        self.inner.clear();
        self.size = 0;
    }
}
