//! A stream for SRT transmission.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use pin_project::pin_project;

use super::buffer::Buffer;
use super::utils::{MessageNumber, Sequence};

#[derive(Debug)]
#[pin_project]
pub struct SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    initial_sequence_number: Sequence,
    buffer: Buffer<(Bytes, MessageNumber)>,
    #[pin]
    stream: S,
    next_message_number: MessageNumber,
}

impl<S> SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    pub fn new(stream: S, size: usize, initial_sequence_number: Sequence) -> Self {
        Self {
            initial_sequence_number,
            stream,
            buffer: Buffer::new(size),
            next_message_number: MessageNumber::new(1),
        }
    }

    pub fn get(&self, seq: Sequence) -> Option<(&Bytes, MessageNumber)> {
        if seq < self.initial_sequence_number {
            None
        } else {
            let msg = (seq - self.initial_sequence_number).get();
            self.buffer.get(msg as usize).map(|(buf, msg)| (buf, *msg))
        }
    }
}

impl<S> Stream for SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    type Item = (Bytes, MessageNumber);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.stream.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(val)) => {
                let msgnum = *this.next_message_number;

                this.buffer.push((val.clone(), msgnum));
                *this.next_message_number += 1;
                Poll::Ready(Some((val, msgnum)))
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
