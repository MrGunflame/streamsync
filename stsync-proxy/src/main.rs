use tokio::net::UdpSocket;

mod proto;
mod rtcp;
mod rtmp;
mod rtp;
mod srt;

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:9999").await.unwrap();

    srt::handshake(socket).await.unwrap();
}
