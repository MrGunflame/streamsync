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
    queue: HashMap<u32, Vec<u8>>,

    is_ordered: bool,
    is_closed: bool,
}

impl<S> OutputSink<S>
where
    S: Sink<Vec<u8>> + Unpin,
{
    pub fn new(sink: S, seqnum: u32) -> Self {
        Self {
            next_seqnum: seqnum,
            sink,
            queue: HashMap::new(),
            is_ordered: true,
            is_closed: false,
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
                match self.queue.remove(&self.next_seqnum) {
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
        self.sink.close().await
    }
}
