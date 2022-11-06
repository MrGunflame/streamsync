//! SRT Output sink
use std::collections::{HashMap, VecDeque};
use std::hint::unreachable_unchecked;
use std::num::Wrapping;
use std::pin::Pin;
use std::task::{ready, Context, Poll, Waker};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::sink::Feed;
use futures::{Future, FutureExt, Sink, Stream, StreamExt};
use tokio::time::Sleep;

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
    next_msgnum: Wrapping<u32>,
    sink: LiveSink<S::Sink>,
    queue: BufferQueue,
    latency: u16,
}

impl<S> OutputSink<S>
where
    S: SessionManager,
{
    /// Creates a new `OutputSink` using `sink` as the underlying [`Sink`].
    pub fn new(sink: LiveSink<S::Sink>, latency: u16) -> Self {
        Self {
            next_msgnum: Wrapping(1),
            sink,
            queue: BufferQueue::new(latency),
            latency,
        }
    }

    pub fn push(&mut self, packet: DataPacket) {
        let msgnum = packet.message_number();

        if self.next_msgnum.0 > msgnum {
            tracing::trace!("Segment {} received too late", msgnum);
            return;
        }

        self.queue.push(msgnum, Instant::now(), packet.data);

        if self.next_msgnum.0 == msgnum {
            tracing::trace!("Segment {} is in order", msgnum);
        } else {
            tracing::trace!(
                "Segment {} is out of order (missing {})",
                msgnum,
                self.next_msgnum
            );
        }
    }

    fn sink(self: Pin<&mut Self>) -> Pin<&mut LiveSink<S::Sink>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }

    fn project(
        self: Pin<&mut Self>,
    ) -> (
        Pin<&mut LiveSink<S::Sink>>,
        Pin<&mut BufferQueue>,
        &mut Wrapping<u32>,
    ) {
        let this = unsafe { self.get_unchecked_mut() };

        let sink = unsafe { Pin::new_unchecked(&mut this.sink) };
        let queue = unsafe { Pin::new_unchecked(&mut this.queue) };
        let next_msgnum = &mut this.next_msgnum;

        (sink, queue, next_msgnum)
    }

    pub fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <Self as Sink<DataPacket>>::Error>> {
        let (mut sink, mut queue, next_msgnum) = self.project();

        ready!(queue.as_mut().poll_ready(cx));

        if let Err(err) = ready!(sink.as_mut().poll_ready(cx)) {
            return Poll::Ready(Err(err));
        }

        let queue = unsafe { queue.get_unchecked_mut() };

        let res = sink.start_send(queue.pop().unwrap().into());

        Poll::Ready(res)
    }
}

impl<S> Sink<DataPacket> for OutputSink<S>
where
    S: SessionManager,
{
    type Error = <LiveSink<S::Sink> as Sink<Bytes>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, packet: DataPacket) -> Result<(), Self::Error> {
        let this = unsafe { self.get_unchecked_mut() };
        this.push(packet);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let (sink, queue, _) = self.project();

        if !queue.is_empty() {
            tracing::debug!("Dropping {} bytes from queue", queue.size);

            let queue = unsafe { queue.get_unchecked_mut() };
            queue.clear();
        }

        sink.poll_close(cx)
    }
}

#[derive(Debug)]
struct BufferQueue {
    queue: VecDeque<(u32, Instant, Vec<u8>)>,
    /// Total size of all buffers combined.
    size: usize,
    latency: Duration,
    sleep: WakeableSleep,
}

impl BufferQueue {
    pub fn new(latency: u16) -> Self {
        tracing::info!("Initialized SrtSink buffer with {}ms latency", latency);

        Self {
            queue: VecDeque::new(),
            size: 0,
            latency: Duration::from_millis(latency as u64),
            sleep: WakeableSleep::new(),
        }
    }

    pub fn push(&mut self, seq: u32, time: Instant, buf: Vec<u8>) {
        // We never store empty buffers.
        if buf.len() != 0 {
            self.size += buf.len();

            let mut index = 0;
            while index < self.queue.len() {
                let elem = self.queue.get(index).unwrap();

                // Insert the element at the position of the missing sequence.
                if seq < elem.0 {
                    self.queue.insert(index, (seq, time, buf));

                    if index == 0 {
                        self.update();
                    }

                    return;
                }

                index += 1;
            }

            self.queue.push_back((seq, time, buf));

            if index == 0 {
                self.update();
            }
        }
    }

    pub fn pop_time(&mut self) -> Option<Instant> {
        let (_, time, _) = self.last()?;
        time.checked_add(self.latency)
    }

    pub fn pop(&mut self) -> Option<Vec<u8>> {
        if self.queue.is_empty() {
            None
        } else {
            self.queue
                .remove(self.queue.len() - 1)
                .map(|(_, _, buf)| buf)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.size = 0;
    }

    fn last(&self) -> Option<&(u32, Instant, Vec<u8>)> {
        if self.queue.is_empty() {
            None
        } else {
            self.queue.get(self.queue.len() - 1)
        }
    }

    fn sleep(self: Pin<&mut Self>) -> Pin<&mut WakeableSleep> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sleep) }
    }

    fn update(&mut self) {
        let time = self.pop_time().unwrap();
        self.sleep.update(time);
    }

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        match self.as_mut().sleep().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                let this = unsafe { self.get_unchecked_mut() };
                this.update();
                Poll::Ready(true)
            }
        }
    }
}

/// A [`Sleep`] future that can be woken manually.
#[derive(Debug)]
struct WakeableSleep {
    waker: Option<Waker>,
    sleep: Option<Sleep>,
}

impl WakeableSleep {
    fn new() -> Self {
        Self {
            waker: None,
            sleep: None,
        }
    }

    /// Updates the [`Sleep`] future in place and wake the future.
    fn update(&mut self, deadline: Instant) {
        let sleep = tokio::time::sleep_until(deadline.into());
        self.sleep = Some(sleep);
        self.wake();
    }

    /// Wake the future manually.
    fn wake(&self) {
        if let Some(waker) = &self.waker {
            waker.wake_by_ref();
        }
    }

    fn project(self: Pin<&mut Self>) -> (&mut Option<Waker>, Option<Pin<&mut Sleep>>) {
        let this = unsafe { self.get_unchecked_mut() };

        let sleep = this
            .sleep
            .as_mut()
            .map(|f| unsafe { Pin::new_unchecked(f) });

        (&mut this.waker, sleep)
    }
}

impl Future for WakeableSleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (waker, sleep) = self.project();

        let update_waker = match &waker {
            Some(waker) => !waker.will_wake(cx.waker()),
            None => true,
        };

        if update_waker {
            *waker = Some(cx.waker().clone());
        }

        match sleep {
            Some(sleep) => sleep.poll(cx),
            None => Poll::Pending,
        }
    }
}
