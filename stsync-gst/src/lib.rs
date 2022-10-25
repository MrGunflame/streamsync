pub mod element;
pub mod format;

use futures::StreamExt;
use gst::{ClockTime, MessageView, State};
pub use gstreamer as gst;
pub use gstreamer_app as gst_app;

use gstreamer::{traits::ElementExt, Element, ElementFactory, Pipeline};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    MissingElement(&'static str),
}

pub trait MediaFormat {
    fn is_audio(&self) -> bool;
    fn is_video(&self) -> bool;
}

pub trait AudioMediaFormat: MediaFormat {}

pub trait VideoMediaFormat: MediaFormat {}

pub trait Source<T>: AsRef<Element>
where
    T: MediaFormat,
{
    fn link<S>(&self, sink: &S)
    where
        S: Sink<T>,
    {
        self.as_ref().link(sink.as_ref()).unwrap();
    }
}

pub trait Sink<T>: AsRef<Element>
where
    T: MediaFormat,
{
}

pub(crate) fn create_element(name: &'static str) -> Result<Element> {
    ElementFactory::make(name, None).map_err(|_| Error::MissingElement(name))
}

pub trait GstElement: AsRef<Element> {
    const NAME: &'static str;
    const PACKAGE: &'static str;

    fn link<T, P>(&self, _other: &T, _pipeline: P) -> Result<()>
    where
        T: GstElement,
        P: AsRef<Pipeline>,
    {
        Ok(())
    }
}

pub async fn run_pipeline(pipeline: &Pipeline) {
    pipeline.set_state(State::Playing).unwrap();

    let mut stream = pipeline.bus().unwrap().stream();

    while let Some(msg) = stream.next().await {
        match msg.view() {
            MessageView::Eos(_) => break,
            MessageView::Error(msg) => {
                panic!("{:?}", msg);
            }
            MessageView::Warning(msg) => {
                tracing::warn!("{:?}", msg);
            }
            _ => (),
        }
    }

    tracing::trace!("Pipeline stopped");
    pipeline.set_state(State::Null).unwrap();
}
