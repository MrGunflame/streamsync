pub mod buffer;
pub mod file;

use std::fmt::{self, Display, Formatter};
use std::num::ParseIntError;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{Sink, Stream};
use snowflaked::Snowflake;
use thiserror::Error;

#[derive(Copy, Clone, Debug, Error)]
pub enum Error {
    #[error("invalid resource id")]
    InvalidResourceId,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("server error")]
    ServerError,
}

pub trait SessionManager: Send + Sync + 'static {
    type Sink: Sink<Bytes> + Send + Sync + Unpin + 'static;
    type Stream: Stream<Item = Bytes> + Send + Sync + Unpin + 'static;

    fn publish(&self, resource_id: Option<ResourceId>) -> Result<LiveSink<Self::Sink>, Error>;

    fn request(&self, resource_id: ResourceId) -> Result<LiveStream<Self::Stream>, Error>;
}

/// A unique identifier for stream.
///
/// A `ResourceId` is an arbitrary 64-bit number. When used in a string form it is lower hex
/// encoded. It implements [`Display`] and [`FromStr`] accordingly.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ResourceId(pub u64);

impl Display for ResourceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

impl FromStr for ResourceId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(s, 16).map(|v| Self(v))
    }
}

impl Snowflake for ResourceId {
    fn from_parts(timestamp: u64, instance: u64, sequence: u64) -> Self {
        Self(u64::from_parts(timestamp, instance, sequence))
    }

    fn timestamp(&self) -> u64 {
        self.0.timestamp()
    }

    fn sequence(&self) -> u64 {
        self.0.sequence()
    }

    fn instance(&self) -> u64 {
        self.0.instance()
    }
}

#[derive(Debug)]
pub struct LiveStream<S>
where
    S: Stream<Item = Bytes>,
{
    resource_id: ResourceId,
    stream: S,
}

impl<S> LiveStream<S>
where
    S: Stream<Item = Bytes>,
{
    pub fn new(resource_id: ResourceId, stream: S) -> Self {
        Self {
            resource_id,
            stream,
        }
    }

    /// Create a pin projection of `self.stream`.
    #[inline]
    fn stream(self: Pin<&mut Self>) -> Pin<&mut S> {
        unsafe { self.map_unchecked_mut(|this| &mut this.stream) }
    }
}

impl<S> Stream for LiveStream<S>
where
    S: Stream<Item = Bytes>,
{
    type Item = Bytes;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream().poll_next(cx)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[derive(Debug)]
pub struct LiveSink<S>
where
    S: Sink<Bytes>,
{
    resource_id: ResourceId,
    sink: S,
}

impl<S> LiveSink<S>
where
    S: Sink<Bytes>,
{
    pub fn new(resource_id: ResourceId, sink: S) -> Self {
        Self { resource_id, sink }
    }

    /// Returns the [`ResourceId`] of this `LiveSink`.
    #[inline]
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Create a pin projection of `self.sink`.
    #[inline]
    fn sink(self: Pin<&mut Self>) -> Pin<&mut S> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }
}

impl<S> Sink<Bytes> for LiveSink<S>
where
    S: Sink<Bytes>,
{
    type Error = S::Error;

    #[inline]
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_ready(cx)
    }

    #[inline]
    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        self.sink().start_send(item)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_flush(cx)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_close(cx)
    }
}
