//! A sink handles incoming data streams.
//!

use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{convert::Infallible, fs::File};

use futures::Sink;
use stsync_gst::element::{AppSrc, AutoVideoSink, DecodeBin};
use stsync_gst::format::PcmVideo;
use stsync_gst::gst::{Buffer, Memory, Pipeline, Sample};
use stsync_gst::gst_app::app_src::AppSrcSink;
use stsync_gst::{run_pipeline, GstElement};

pub struct SessionId(pub u64);

pub trait MultiSink: Clone + Send + Sync + 'static {
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

        let n2 = name.clone();
        std::thread::spawn(move || {
            std::process::Command::new("gst-launch-1.0")
                .args([
                    "filesrc",
                    &format!("location={}", n2),
                    "!",
                    "decodebin",
                    "!",
                    "autovideosink",
                ])
                .spawn()
                .unwrap();
        });

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

#[derive(Copy, Clone, Debug)]
pub struct NullSink;

impl Sink<Vec<u8>> for NullSink {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, _: Vec<u8>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiveMultiSink;

impl MultiSink for LiveMultiSink {
    type Sink = LiveSink;

    fn attach(&self, id: SessionId) -> Self::Sink {
        stsync_gst::gst::init().unwrap();
        let pipeline = Pipeline::new(None);

        // This is a lie but it doesn't matter here.
        let appsrc: AppSrc<PcmVideo> = AppSrc::new().unwrap();
        let decodebin = DecodeBin::new().unwrap();
        let autovideosink = AutoVideoSink::new().unwrap();

        let sink = appsrc.sink();

        tokio::task::spawn(async move {
            appsrc.link(&decodebin, &pipeline).unwrap();
            decodebin.link(&autovideosink, &pipeline).unwrap();

            println!("Created pipeline");
            run_pipeline(&pipeline).await;
            println!("Stopped pipeline");
        });

        LiveSink { sink }
    }

    fn dettach(&self, id: SessionId) {}
}

pub struct LiveSink {
    sink: AppSrcSink,
}

impl LiveSink {
    fn sink(self: Pin<&mut Self>) -> Pin<&mut AppSrcSink> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }
}

impl Sink<Vec<u8>> for LiveSink {
    type Error = <AppSrcSink as Sink<Sample>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        let mut buffer = Buffer::new();
        let mem = Memory::from_slice(item);
        buffer.make_mut().append_memory(mem);

        let sample = Sample::builder().buffer(&buffer).build();
        let res = self.sink().start_send(sample);
        res
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink().poll_close(cx)
    }
}
