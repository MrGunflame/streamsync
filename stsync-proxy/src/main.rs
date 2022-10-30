use session::{buffer::BufferSessionManager, file::FileSessionManager};
use srt::config::Config;

mod http;
mod metrics;
mod proto;
mod rtcp;
mod rtmp;
mod rtp;
mod session;
mod srt;
mod state;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let manager = BufferSessionManager::new();

    let (state, fut) = srt::server::serve("[::]:9999", manager, Config::default());

    tokio::task::spawn(async move {
        http::serve(state).await;
    });

    fut.await.unwrap();
}
