use tokio::net::UdpSocket;

mod proto;
mod rtcp;
mod rtmp;
mod rtp;
mod sink;
mod srt;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    srt::server::serve("0.0.0.0:9999").await.unwrap();
}
