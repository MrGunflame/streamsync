use session::{buffer::BufferSessionManager, file::FileSessionManager};
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

    let manager = BufferSessionManager::new();

    srt::server::serve("[::]:9999", manager, Config::default())
        .await
        .unwrap();
}
