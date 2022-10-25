use gstreamer::{
    ffi::GstPlugin,
    prelude::{GstBinExtManual, ObjectExt},
    traits::ElementExt,
    Element,
};

use crate::{create_element, GstElement, MediaFormat, Result, Source};

/// A source element that streams content from a file. `FileSrc` can output any [`MediaFormat`]
/// depending on the file contents.
#[derive(Clone, Debug)]
pub struct FileSrc {
    element: Element,
}

impl FileSrc {
    pub fn new(location: &str) -> Result<Self> {
        let element = create_element("filesrc")?;
        element.set_property("location", location);

        Ok(Self { element })
    }
}

impl AsRef<Element> for FileSrc {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl GstElement for FileSrc {
    const NAME: &'static str = "filesrc";
    const PACKAGE: &'static str = "gstreamer";

    fn link<T, P>(&self, other: &T, pipeline: P) -> Result<()>
    where
        T: GstElement,
        P: AsRef<gstreamer::Pipeline>,
    {
        pipeline
            .as_ref()
            .add_many(&[self.as_ref(), other.as_ref()])
            .unwrap();

        self.element.link(other.as_ref()).unwrap();
        Ok(())
    }
}

impl<T> Source<T> for FileSrc where T: MediaFormat {}
