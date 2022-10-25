use crate::{AudioMediaFormat, MediaFormat, VideoMediaFormat};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Http;

pub struct PcmAudio;

pub struct Hls;

impl MediaFormat for PcmAudio {
    fn is_audio(&self) -> bool {
        true
    }

    fn is_video(&self) -> bool {
        false
    }
}

impl AudioMediaFormat for PcmAudio {}

pub struct PcmVideo;

impl MediaFormat for PcmVideo {
    fn is_audio(&self) -> bool {
        false
    }

    fn is_video(&self) -> bool {
        true
    }
}

impl VideoMediaFormat for PcmVideo {}
