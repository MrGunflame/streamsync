//! A stream for SRT transmission.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;

use super::buffer::Buffer;

#[derive(Debug)]
pub struct SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    initial_sequence_number: u32,
    stream: S,
    buffer: Buffer<Bytes>,
}

impl<S> SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    pub fn new(stream: S, size: usize, initial_sequence_number: u32) -> Self {
        Self {
            initial_sequence_number,
            stream,
            buffer: Buffer::new(size),
        }
    }

    pub fn get(&self, seq: u32) -> Option<&Bytes> {
        if seq < self.initial_sequence_number {
            None
        } else {
            let index = seq.wrapping_sub(self.initial_sequence_number);
            self.buffer.get(index as usize)
        }
    }

    fn project(self: Pin<&mut Self>) -> (Pin<&mut S>, &mut Buffer<Bytes>) {
        let this = unsafe { self.get_unchecked_mut() };

        let stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        let buffer = &mut this.buffer;

        (stream, buffer)
    }
}

impl<S> Stream for SrtStream<S>
where
    S: Stream<Item = Bytes>,
{
    type Item = Bytes;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (stream, buffer) = self.project();

        match stream.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(val)) => {
                buffer.push(val.clone());
                Poll::Ready(Some(val))
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
