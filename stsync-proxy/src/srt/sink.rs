//! Output sinks
use std::collections::HashMap;
use std::hint::unreachable_unchecked;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use bytes::Bytes;
use futures::Sink;

use crate::session::{LiveSink, SessionManager};

use super::DataPacket;

/// A [`Sink`] that receives [`DataPacket`]s and converts them back into data.
///
/// `OutputSink` wraps around a regular sink `S` accepting the data of the underlying network
/// stream. An `OutputSink` is responsible for reassembling the stream of the sender by reordering
/// out-of-order segments, dropping duplicate and too-late packets.
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
    poll_state: PollState,
}

impl<S> OutputSink<S>
where
    S: SessionManager,
{
    /// Creates a new `OutputSink` using `sink` as the underlying [`Sink`].
    pub fn new(sink: LiveSink<S::Sink>) -> Self {
        Self {
            next_msgnum: 1,
            sink,
            queue: BufferQueue::new(),
            skip_counter: 0,
            poll_state: PollState::Ready,
        }
    }

    fn sink(self: Pin<&mut Self>) -> Pin<&mut LiveSink<S::Sink>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }

    fn project(
        self: Pin<&mut Self>,
    ) -> (
        Pin<&mut LiveSink<S::Sink>>,
        &mut BufferQueue,
        &mut u32,
        &mut u8,
        &mut PollState,
    ) {
        let this = unsafe { self.get_unchecked_mut() };

        let sink = unsafe { Pin::new_unchecked(&mut this.sink) };
        let queue = &mut this.queue;
        let next_msgnum = &mut this.next_msgnum;
        let skip_counter = &mut this.skip_counter;
        let poll_state = &mut this.poll_state;

        (sink, queue, next_msgnum, skip_counter, poll_state)
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <Self as Sink<DataPacket>>::Error>> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Write));

        let (mut sink, queue, next_msgnum, _, poll_state) = self.project();

        if let Err(err) = ready!(sink.as_mut().poll_ready(cx)) {
            return Poll::Ready(Err(err));
        }

        match queue.remove(*next_msgnum) {
            Some(buf) => {
                sink.start_send(buf.into())?;

                *poll_state = PollState::Write;
                Poll::Ready(Ok(()))
            }
            None => {
                *poll_state = PollState::Ready;
                Poll::Ready(Ok(()))
            }
        }
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

        let (mut sink, queue, next_msgnum, skip_counter, poll_state) = self.project();

        if let Err(err) = ready!(sink.as_mut().poll_ready(cx)) {
            return Poll::Ready(Err(err));
        }

        while *next_msgnum <= target {
            if let Some(buf) = queue.remove(*next_msgnum) {
                if let Err(err) = sink.as_mut().start_send(buf.into()) {
                    return Poll::Ready(Err(err));
                }

                return Poll::Ready(Ok(()));
            }

            *next_msgnum += 1;
        }

        // Catchup done
        *skip_counter = 0;
        *poll_state = PollState::Ready;
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

        if self.next_msgnum > msgnum {
            tracing::trace!("Segment {} received too late", msgnum);
            return Ok(());
        }

        if self.next_msgnum == msgnum {
            self.skip_counter = self.skip_counter.saturating_sub(1);

            tracing::trace!("Segment {} is in order", msgnum);

            self.as_mut().sink().start_send(packet.data.into())?;

            self.poll_state = PollState::Write;
        } else {
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
        self.sink().poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.queue.is_empty() {
            tracing::debug!("Dropping {} bytes from queue", self.queue.size);
            self.queue.clear();
        }

        self.sink().poll_close(cx)
    }
}

#[derive(Clone, Debug, Default)]
struct BufferQueue {
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

#[derive(Debug)]
enum PollState {
    Ready,
    Write,
    SkipAhead { target: u32 },
}
