use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{io, Sink, Stream};
use snowflaked::sync::Generator;
use tokio::time::Interval;

use super::{Error, LiveSink, LiveStream, ResourceId, SessionManager};

#[derive(Debug)]
pub struct FileSessionManager {
    resource_id: Generator,
}

impl FileSessionManager {
    pub fn new() -> Self {
        Self {
            resource_id: Generator::new(0),
        }
    }
}

impl SessionManager for FileSessionManager {
    type Sink = FileSink;
    type Stream = FileStream;

    fn publish(&self, resource_id: Option<ResourceId>) -> Result<LiveSink<Self::Sink>, Error> {
        match resource_id {
            Some(_) => Err(Error::InvalidResourceId),
            None => {
                let resource_id = self.resource_id.generate();

                let file = match File::create(format!("{}.ts", resource_id)) {
                    Ok(file) => file,
                    Err(err) => {
                        tracing::error!("Failed to open file: {}", err);
                        return Err(Error::ServerError);
                    }
                };

                let sink = FileSink { file };

                Ok(LiveSink::new(resource_id, sink))
            }
        }
    }

    fn request(&self, resource_id: ResourceId) -> Result<LiveStream<Self::Stream>, Error> {
        let file = match File::open(format!("{}.ts", resource_id)) {
            Ok(file) => file,
            Err(err) => {
                tracing::debug!("Failed to open file {}: ", err);
                return Err(Error::InvalidResourceId);
            }
        };

        Ok(LiveStream::new(
            resource_id,
            FileStream {
                file,
                is_done: false,
            },
        ))
    }
}

pub struct FileSink {
    file: File,
}

impl Sink<Bytes> for FileSink {
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        self.file.write_all(&item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
pub struct FileStream {
    file: File,
    is_done: bool,
}

impl Stream for FileStream {
    type Item = Bytes;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.is_done {
            return Poll::Ready(None);
        }

        let mut buf = vec![0; 188 * 7];
        if let Err(err) = self.file.read_exact(&mut buf) {
            tracing::info!("File reached EOF");
            self.is_done = true;

            if err.kind() != ErrorKind::UnexpectedEof {
                panic!("Failed to read file: {}", err);
            }
        }

        Poll::Ready(Some(buf.into()))
    }
}
