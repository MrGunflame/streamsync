use std::marker::PhantomData;

use gstreamer::{
    prelude::{Cast, GstBinExtManual},
    traits::ElementExt,
    ClockTime, Element, Format,
};
use gstreamer_app::app_src::AppSrcSink;

use crate::{create_element, GstElement, MediaFormat, Result, Source};

#[derive(Clone, Debug)]
pub struct AppSrc<T>
where
    T: MediaFormat,
{
    element: gstreamer_app::AppSrc,
    _marker: PhantomData<T>,
}

impl<T> AppSrc<T>
where
    T: MediaFormat,
{
    pub fn new() -> Result<Self> {
        let element = create_element("appsrc")?;
        let element: gstreamer_app::AppSrc = unsafe { element.unsafe_cast() };

        element.set_format(Format::Time);
        element.set_is_live(true);
        // element.set_do_timestamp(true);
        element.set_latency(Some(ClockTime::SECOND), Some(ClockTime::SECOND * 60));

        Ok(Self {
            element,
            _marker: PhantomData,
        })
    }

    pub fn sink(&self) -> AppSrcSink {
        self.element.sink()
    }
}

impl<T> AsRef<Element> for AppSrc<T>
where
    T: MediaFormat,
{
    fn as_ref(&self) -> &Element {
        self.element.as_ref()
    }
}

impl<T> GstElement for AppSrc<T>
where
    T: MediaFormat,
{
    const NAME: &'static str = "appsrc";
    const PACKAGE: &'static str = "base";

    fn link<E, P>(&self, other: &E, pipeline: P) -> Result<()>
    where
        E: GstElement,
        P: AsRef<gstreamer::Pipeline>,
    {
        let pipeline = pipeline.as_ref();
        pipeline.add_many(&[self.as_ref(), other.as_ref()]).unwrap();

        self.as_ref().link(other.as_ref()).unwrap();

        Ok(())
    }
}

impl<T> Source<T> for AppSrc<T> where T: MediaFormat {}
