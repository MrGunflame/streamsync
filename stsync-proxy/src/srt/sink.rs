//! SRT Output sink
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::future::Future;
use std::marker::PhantomPinned;
use std::num::Wrapping;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::{pin_mut, Sink};
use pin_project::pin_project;
use tokio::time::{sleep_until, Sleep};

use crate::session::{LiveSink, SessionManager};

use super::utils::{MessageNumber, Sequence};
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
    queue: SegmentQueue,
    #[pin]
    sink: LiveSink<S::Sink>,
}

impl<S> OutputSink<S>
where
    S: SessionManager,
{
    /// Creates a new `OutputSink` using `sink` as the underlying [`Sink`].
    pub fn new(
        sink: LiveSink<S::Sink>,
        start: Instant,
        latency: Duration,
        buffer_size: usize,
    ) -> Self {
        Self {
            next_msgnum: Wrapping(1),
            sink,
            queue: SegmentQueue::new(start, latency, buffer_size),
        }
    }

    /// Returns the remaining capacity in the output buffer.
    #[inline]
    pub fn buffer_left(&self) -> usize {
        8192 - self.queue.len()
    }

    /// Write to output sink with latency.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <S::Sink as Sink<Bytes>>::Error>> {
        let mut this = self.project();

        ready!(this.sink.as_mut().poll_ready(cx))?;

        let fut = this.queue.take();
        pin_mut!(fut);
        let segment = ready!(fut.poll(cx));

        if let Some(segment) = segment {
            this.sink.start_send(segment.payload)?;
        }

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
            if self.queue.is_empty() || self.as_mut().poll_write(cx).is_pending() {
                break;
            }
        }
        // self.poll_write(cx);

        // TODO: Implement a fixed buffer size.
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, packet: DataPacket) -> Result<(), Self::Error> {
        self.queue.push(packet);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Drain the queue.
        while !self.queue.is_empty() {
            ready!(self.as_mut().poll_write(cx))?;
        }

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

/// A queue of [`Segment`]s to be written.
#[derive(Debug)]
struct SegmentQueue {
    queue: BTreeSet<Segment>,
    /// Total size of all buffers combined.
    size: usize,
    start: Instant,
    latency: Duration,
    buffer_size: usize,
}

impl SegmentQueue {
    pub fn new(start: Instant, latency: Duration, buffer_size: usize) -> Self {
        Self {
            queue: BTreeSet::new(),
            size: 0,
            start,
            latency,
            buffer_size,
        }
    }

    pub fn push(&mut self, mut packet: DataPacket) {
        // Prevent memory exhaustion from slow receivers or attacks.
        if self.len() > self.buffer_size {
            return;
        }

        let sequence = Sequence::new(packet.packet_sequence_number());
        let message_number = packet.message_number();
        let delivery_time = self.start + packet.header.timestamp.to_duration() + self.latency;

        self.size += packet.data.len();
        self.queue.insert(Segment {
            sequence,
            message_number,
            delivery_time,
            payload: packet.data,
        });
    }

    pub fn first(&mut self) -> Option<&'_ Segment> {
        self.queue.first()
    }

    pub fn pop_first(&mut self) -> Option<Segment> {
        let segment = self.queue.pop_first()?;
        self.size -= segment.payload.len();
        Some(segment)
    }

    /// Returns a future that completes once the next segment can be taken.
    ///
    /// **Note that the queue will only keep track of the most recent waker.**
    pub fn take(&mut self) -> Take<'_> {
        Take {
            queue: self,
            sleep: None,
            _pin: PhantomPinned,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.size = 0;
    }
}

#[derive(Clone, Debug)]
struct Segment {
    sequence: Sequence,
    message_number: MessageNumber,
    delivery_time: Instant,
    payload: Bytes,
}

impl PartialEq for Segment {
    fn eq(&self, other: &Self) -> bool {
        self.message_number == other.message_number && self.delivery_time == other.delivery_time
    }
}

impl Eq for Segment {}

impl PartialOrd for Segment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Segment {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.message_number.cmp(&other.message_number) {
            Ordering::Less => Ordering::Less,
            Ordering::Equal => self.delivery_time.cmp(&other.delivery_time),
            Ordering::Greater => Ordering::Greater,
        }
    }
}

#[pin_project]
struct Take<'a> {
    queue: &'a mut SegmentQueue,
    sleep: Option<Sleep>,
    _pin: PhantomPinned,
}

impl<'a> Future for Take<'a> {
    type Output = Option<Segment>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let now = Instant::now();

        let deadline;
        if let Some(seg) = this.queue.first() {
            if seg.delivery_time <= now {
                // FIXME: pop_first == None is unreachable.
                return Poll::Ready(this.queue.pop_first());
            } else {
                deadline = seg.delivery_time;
            }
        } else {
            // The queue is empty.
            return Poll::Ready(None);
        }

        if this.sleep.is_none() {
            let sleep = sleep_until(deadline.into());
            *this.sleep = Some(sleep);
        }

        tracing::trace!("Ready in {:?}", deadline - now);

        let sleep = this.sleep.as_mut().unwrap();
        // SAFETY: `self` is `!Unpin`. It is never moved.
        let sleep = unsafe { Pin::new_unchecked(sleep) };
        ready!(sleep.poll(cx));

        // Since we only ever store one waiter the segment is guaranteed to still be
        // untouched when the timer expires.
        Poll::Ready(this.queue.pop_first())
    }
}
