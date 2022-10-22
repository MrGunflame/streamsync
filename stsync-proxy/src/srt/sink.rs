//! Output sinks
use std::collections::HashMap;

use futures::{Sink, SinkExt};

use super::state::Connection;
use super::DataPacket;

#[derive(Debug)]
pub struct OutputSink<S>
where
    S: Sink<Vec<u8>>,
{
    /// Sequence number of the next expected segment.
    next_seqnum: u32,
    sink: S,
    queue: BufferQueue,
    skip_counter: u8,

    /// Reference to the connection owning this sink. The connection is always guaranteed to
    /// outlive the sink it owns. Since [`Connection`] usually comes wrapped in an Arc, it must
    /// not dereferenced mutably.
    conn: *const Connection<S>,
}

impl<S> OutputSink<S>
where
    S: Sink<Vec<u8>> + Unpin,
{
    pub fn new(conn: &Connection<S>, sink: S, seqnum: u32) -> Self {
        Self {
            next_seqnum: seqnum,
            sink,
            queue: BufferQueue::new(),
            skip_counter: 0,
            conn: conn as *const _,
        }
    }

    pub async fn push(&mut self, packet: DataPacket) -> Result<(), S::Error> {
        let seqnum = packet.packet_sequence_number();

        // We already assumed the packets to be lost and skipped ahead.
        if self.next_seqnum > seqnum {
            tracing::trace!("Segment {} received too late", seqnum);
            return Ok(());
        }

        if self.next_seqnum == seqnum {
            tracing::trace!("Segment {} is in order", seqnum);
            self.skip_counter = self.skip_counter.saturating_sub(1);

            self.sink.feed(packet.data).await?;
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
            self.skip_counter += 1;

            tracing::debug!(
                "Segment {} is out of order (missing {}) (skip ahead in {})",
                seqnum,
                self.next_seqnum,
                self.skip_counter,
            );

            self.queue.insert(seqnum, packet.data);

            if self.skip_counter >= 20 {
                self.skip_counter = 0;
                tracing::trace!("Skip ahead (lost 20 segments)");
                let mut num_catched = 0;
                while self.next_seqnum <= seqnum {
                    if let Some(buf) = self.queue.remove(self.next_seqnum) {
                        self.sink.feed(buf).await?;
                        num_catched += 1;
                    }

                    self.next_seqnum += 1;
                }

                tracing::trace!("Recovered {} segments", num_catched);

                // SAFETY: The connection is owning this sink. See struct def for details.
                unsafe {
                    let conn = &*self.conn;
                    conn.metrics.packets_dropped.add(20 - num_catched);
                }
            }
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), S::Error> {
        tracing::debug!("Dropping {} bytes from queue", self.queue.size);
        self.queue.clear();

        self.sink.close().await
    }
}

unsafe impl<S> Send for OutputSink<S> where S: Sink<Vec<u8>> {}
unsafe impl<S> Sync for OutputSink<S> where S: Sink<Vec<u8>> {}

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
