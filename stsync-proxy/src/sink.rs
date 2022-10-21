//! A sink handles incoming data streams.
//!

use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{convert::Infallible, fs::File};

use futures::Sink;
pub struct SessionId(pub u64);

pub trait MultiSink: Clone + Send + Sync {
    type Sink: futures::Sink<Vec<u8>> + Send + Sync + Unpin;

    fn attach(&self, id: SessionId) -> Self::Sink;

    fn dettach(&self, id: SessionId);
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileMultiSink;

impl MultiSink for FileMultiSink {
    type Sink = FileSink;

    fn attach(&self, id: SessionId) -> Self::Sink {
        let name = format!("{}.ts", id.0);
        tracing::debug!("Attachin to {}", name);

        FileSink {
            file: File::create(name).unwrap(),
        }
    }

    fn dettach(&self, _: SessionId) {}
}

#[derive(Debug)]
pub struct FileSink {
    file: File,
}

impl FileSink {}

impl Sink<Vec<u8>> for FileSink {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        self.file.write_all(&item).unwrap();
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
