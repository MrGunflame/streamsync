use session::file::FileSessionManager;
use sink::{FileMultiSink, LiveMultiSink};
use srt::config::Config;

mod proto;
mod rtcp;
mod rtmp;
mod rtp;
mod session;
pub mod sink;
mod srt;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let manager = FileSessionManager::new();

    srt::server::serve("[::]:9999", manager, Config::default())
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use stsync_gst::{
        element::{AutoVideoSink, DecodeBin, FileSrc},
        gst::Pipeline,
        run_pipeline, GstElement,
    };

    #[tokio::test]
    async fn test_pl() {
        stsync_gst::gst::init().unwrap();
        pretty_env_logger::init();

        let pipeline = Pipeline::new(None);

        let filesrc = FileSrc::new("1.ts").unwrap();
        let decodebin = DecodeBin::new().unwrap();
        let autovideosink = AutoVideoSink::new().unwrap();

        filesrc.link(&decodebin, &pipeline).unwrap();
        decodebin.link(&autovideosink, &pipeline).unwrap();

        run_pipeline(&pipeline).await;
    }
}
