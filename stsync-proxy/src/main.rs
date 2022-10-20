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

    let socket = UdpSocket::bind("0.0.0.0:9999").await.unwrap();

    srt::server::serve(socket).await.unwrap();
}
