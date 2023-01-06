//! A stream for SRT transmission.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use futures::Stream;
use pin_project::pin_project;

use super::buffer::Buffer;
use super::proto::Timestamp;
use super::utils::{MessageNumber, Sequence};

#[derive(Debug)]
#[pin_project]
pub struct SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    initial_sequence_number: Sequence,
    // Any retransmissions MUST retain the same timestmap.
    buffer: Buffer<(Bytes, Timestamp, MessageNumber)>,
    #[pin]
    stream: S,
    next_message_number: MessageNumber,
    start_time: Instant,
}

impl<S> SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    pub fn new(
        stream: S,
        size: usize,
        initial_sequence_number: Sequence,
        start_time: Instant,
    ) -> Self {
        Self {
            initial_sequence_number,
            stream,
            buffer: Buffer::new(size),
            next_message_number: MessageNumber::new(1),
            start_time,
        }
    }

    pub fn get(&self, seq: Sequence) -> Option<(&Bytes, Timestamp, MessageNumber)> {
        if seq < self.initial_sequence_number {
            None
        } else {
            let msg = (seq - self.initial_sequence_number).get();
            self.buffer
                .get(msg as usize)
                .map(|(buf, ts, msg)| (buf, *ts, *msg))
        }
    }

    pub fn update_start(&mut self, instant: Instant) {
        self.start_time = instant;
    }
}

impl<S> Stream for SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    type Item = (Bytes, Timestamp, MessageNumber);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.stream.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(val)) => {
                let msgnum = *this.next_message_number;
                let ts = Timestamp::from_start(*this.start_time);

                this.buffer.push((val.clone(), ts, msgnum));
                *this.next_message_number += 1;
                Poll::Ready(Some((val, ts, msgnum)))
            }
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

impl<S> AsRef<S> for SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    #[inline]
    fn as_ref(&self) -> &S {
        &self.stream
    }
}
