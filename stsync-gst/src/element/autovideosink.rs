use gstreamer::Element;

use crate::format::PcmVideo;
use crate::{create_element, GstElement, Result, Sink};

#[derive(Clone, Debug)]
pub struct AutoVideoSink {
    element: Element,
}

impl AutoVideoSink {
    pub fn new() -> Result<Self> {
        let element = create_element("autovideosink")?;

        Ok(Self { element })
    }
}

impl AsRef<Element> for AutoVideoSink {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl GstElement for AutoVideoSink {
    const NAME: &'static str = "autovideosink";
    const PACKAGE: &'static str = "good";
}

impl Sink<PcmVideo> for AutoVideoSink {}
