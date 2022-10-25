use gstreamer::{
    element_warning,
    glib::clone::Downgrade,
    traits::{ElementExt, GstBinExt, GstObjectExt, PadExt},
    CoreError, Element, Pipeline,
};
use tracing::{event, trace_span, Level, Span};

use crate::{
    create_element,
    format::{PcmAudio, PcmVideo},
    GstElement, MediaFormat, Result, Sink, Source,
};

#[derive(Clone, Debug)]
pub struct DecodeBin {
    element: Element,
    span: Span,
}

impl DecodeBin {
    pub fn new() -> Result<Self> {
        let element = create_element("decodebin")?;

        let span = trace_span!(target: "decodebin", "");

        Ok(Self { element, span })
    }
}

impl AsRef<Element> for DecodeBin {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl GstElement for DecodeBin {
    const NAME: &'static str = "decodebin";
    const PACKAGE: &'static str = "base";

    fn link<T, P>(&self, other: &T, pipeline: P) -> Result<()>
    where
        T: GstElement,
        P: AsRef<Pipeline>,
    {
        let sink = other.as_ref().clone();

        let span = self.span.clone();

        let pipeline = pipeline.as_ref().downgrade();
        self.element.connect_pad_added(move |this, src_pad| {
            println!("{:?}", src_pad);
            event!(parent: &span, Level::TRACE, "connect_pad_added");

            let pipeline = match pipeline.upgrade() {
                Some(pipeline) => pipeline,
                None => return,
            };

            let (is_audio, is_video) = {
                let media_type = src_pad.current_caps().and_then(|caps| {
                    caps.structure(0).map(|s| {
                        let name = s.name();
                        (name.starts_with("audio/"), name.starts_with("video/"))
                    })
                });

                match media_type {
                    None => {
                        element_warning!(
                            this,
                            CoreError::Negotiation,
                            ("Failed to get media type from pad {}", src_pad.name())
                        );

                        return;
                    }
                    Some(media_type) => media_type,
                }
            };

            // TODO: impl
            if is_audio {
                return;
            }

            pipeline.add(&sink).unwrap();

            sink.sync_state_with_parent().unwrap();

            let sink_pad = sink.static_pad("sink").unwrap();
            src_pad.link(&sink_pad).unwrap();
        });

        Ok(())
    }
}

impl<T> Source<T> for DecodeBin where T: MediaFormat {}

impl Sink<PcmAudio> for DecodeBin {}

impl Sink<PcmVideo> for DecodeBin {}
