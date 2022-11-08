//! SRT Output sink
use std::collections::HashMap;
use std::hint::unreachable_unchecked;
use std::num::Wrapping;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use bytes::Bytes;
use futures::Sink;
use pin_project::pin_project;

use crate::session::{LiveSink, SessionManager};

use super::DataPacket;

/// A [`Sink`] that receives [`DataPacket`]s and converts them back into data.
///
/// `OutputSink` wraps around a regular sink `S` accepting the data of the underlying network
/// stream. An `OutputSink` is responsible for reassembling the stream of the sender by reordering
/// out-of-order segments, dropping duplicate and too-late packets.
#[derive(Debug)]
#[pin_project]
pub struct OutputSink<S>
where
    S: SessionManager,
{
    /// Sequence number of the next expected segment.
    next_msgnum: Wrapping<u32>,
    queue: BufferQueue,
    skip_counter: u8,
    poll_state: PollState,
    #[pin]
    sink: LiveSink<S::Sink>,
}

impl<S> OutputSink<S>
where
    S: SessionManager,
{
    /// Creates a new `OutputSink` using `sink` as the underlying [`Sink`].
    pub fn new(sink: LiveSink<S::Sink>) -> Self {
        Self {
            next_msgnum: Wrapping(1),
            sink,
            queue: BufferQueue::new(),
            skip_counter: 0,
            poll_state: PollState::Ready,
        }
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <Self as Sink<DataPacket>>::Error>> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Write));

        let mut this = self.project();

        if let Err(err) = ready!(this.sink.as_mut().poll_ready(cx)) {
            return Poll::Ready(Err(err));
        }

        match this.queue.remove(this.next_msgnum.0) {
            Some(buf) => {
                this.sink.start_send(buf.into())?;
                *this.next_msgnum += 1;

                *this.poll_state = PollState::Write;
            }
            None => {
                *this.poll_state = PollState::Ready;
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_skip_ahead(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <Self as Sink<DataPacket>>::Error>> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::SkipAhead { .. }));

        let target = match self.poll_state {
            PollState::SkipAhead { target } => target,
            _ => unsafe { unreachable_unchecked() },
        };

        let mut this = self.project();

        if let Err(err) = ready!(this.sink.as_mut().poll_ready(cx)) {
            return Poll::Ready(Err(err));
        }

        while this.next_msgnum.0 <= target {
            if let Some(buf) = this.queue.remove(this.next_msgnum.0) {
                if let Err(err) = this.sink.as_mut().start_send(buf.into()) {
                    return Poll::Ready(Err(err));
                }

                return Poll::Ready(Ok(()));
            }

            *this.next_msgnum += 1;
        }

        // Catchup done
        *this.skip_counter = 0;
        *this.poll_state = PollState::Ready;
        Poll::Ready(Ok(()))
    }
}

impl<S> Sink<DataPacket> for OutputSink<S>
where
    S: SessionManager,
{
    type Error = <LiveSink<S::Sink> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            match self.poll_state {
                PollState::Ready => return Poll::Ready(Ok(())),
                PollState::Write => {
                    ready!(self.as_mut().poll_write(cx))?;
                }
                PollState::SkipAhead { .. } => {
                    ready!(self.as_mut().poll_skip_ahead(cx))?;
                }
            }
        }
    }

    fn start_send(mut self: Pin<&mut Self>, packet: DataPacket) -> Result<(), Self::Error> {
        let msgnum = packet.message_number();

        if self.next_msgnum.0 > msgnum {
            tracing::trace!("Segment {} received too late", msgnum);
            return Ok(());
        }

        if self.next_msgnum.0 == msgnum {
            self.skip_counter = self.skip_counter.saturating_sub(1);
            self.next_msgnum += 1;

            tracing::trace!("Segment {} is in order", msgnum);

            let this = self.project();

            this.sink.start_send(packet.data.into())?;

            *this.poll_state = PollState::Write;
        } else {
            tracing::trace!(
                "Segment {} is out of order (missing {})",
                msgnum,
                self.next_msgnum
            );
            self.skip_counter += 1;

            self.queue.insert(msgnum, packet.data);

            if self.skip_counter >= 40 {
                tracing::trace!("Skip ahead (HEAD {} => {})", self.next_msgnum, msgnum);

                self.poll_state = PollState::SkipAhead { target: msgnum };
            }
        }

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.sink.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if !this.queue.is_empty() {
            tracing::debug!("Dropping {} bytes from queue", this.queue.size);
            this.queue.clear();
        }

        this.sink.poll_close(cx)
    }
}

#[derive(Clone, Debug, Default)]
struct BufferQueue {
    inner: HashMap<u32, Bytes>,
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

    pub fn insert(&mut self, seq: u32, buf: Bytes) {
        // We never store empty buffers.
        if buf.len() != 0 {
            self.size += buf.len();
            self.inner.insert(seq, buf);
        }
    }

    pub fn remove(&mut self, seq: u32) -> Option<Bytes> {
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

#[derive(Debug)]
enum PollState {
    Ready,
    Write,
    SkipAhead { target: u32 },
}
