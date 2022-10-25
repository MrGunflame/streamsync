use gstreamer::{
    traits::{ElementExt, PadExt},
    Element,
};
use tracing::{event, trace_span, Level, Span};

use crate::{create_element, GstElement, Result};

#[derive(Clone, Debug)]
pub struct HlsDemux {
    element: Element,
    span: Span,
}

impl HlsDemux {
    pub fn new() -> Result<Self> {
        let element = create_element("hlsdemux")?;
        let span = trace_span!(target: "hlsdemux", "");

        Ok(Self { element, span })
    }
}

impl AsRef<Element> for HlsDemux {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl GstElement for HlsDemux {
    const NAME: &'static str = "hlsdemux";
    const PACKAGE: &'static str = "bad";

    fn link<T, P>(&self, other: &T, _pipeline: P) -> Result<()>
    where
        T: GstElement,
        P: AsRef<gstreamer::Pipeline>,
    {
        let sink = other.as_ref().clone();
        let span = self.span.clone();

        self.element.connect_pad_added(move |_this, src_pad| {
            event!(parent: &span, Level::TRACE, "connect_pad_added");

            let sink_pad = sink.static_pad("sink").unwrap();
            src_pad.link(&sink_pad).unwrap();
        });

        self.element.sync_state_with_parent().unwrap();

        Ok(())
    }
}
