#![deny(unsafe_op_in_unsafe_fn)]

use session::{buffer::BufferSessionManager, file::FileSessionManager};
use srt::{config::Config, server::Server};
use tokio::runtime::Builder;

mod http;
mod metrics;
mod proto;
mod rtcp;
mod rtmp;
mod rtp;
mod session;
mod srt;
mod state;
mod utils;

fn main() {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    rt.block_on(async_main());
}

async fn async_main() {
    pretty_env_logger::init();

    let manager = BufferSessionManager::new();

    let server = Server::new(manager, Config::default()).unwrap();

    let state = server.state.clone();
    tokio::task::spawn(async move {
        http::serve(state).await;
    });

    server.await.unwrap();
}
