use sink::FileMultiSink;
use srt::config::Config;
use tokio::net::UdpSocket;

mod proto;
mod rtcp;
mod rtmp;
mod rtp;
pub mod sink;
mod srt;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    srt::server::serve("0.0.0.0:9999", FileMultiSink, Config::default())
        .await
        .unwrap();
}
