#![deny(unsafe_op_in_unsafe_fn)]

use clap::Parser;
use config::Config;
use session::{buffer::BufferSessionManager, file::FileSessionManager};
use srt::server::Server;
use state::State;
use tokio::runtime::Builder;

mod config;
mod database;
mod http;
mod metrics;
mod proto;
mod session;
mod signal;
mod srt;
mod state;
mod utils;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_name = "FILE", default_value = "config.toml")]
    config: String,
}

fn main() {
    signal::init();
    pretty_env_logger::init();

    let args = Args::parse();

    let config = match Config::from_file(&args.config) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("Failed to load config file: {}", err);
            return;
        }
    };

    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    rt.block_on(async_main(config));
}

async fn async_main(config: Config) {
    let manager = BufferSessionManager::new();

    let server = Server::new(manager, config.srt.clone()).unwrap();
    let state = State::new(server.state.clone());

    if config.srt.enabled {
        tokio::task::spawn(async move {
            server.await.unwrap();
        });
    }

    if config.http.enabled {
        tokio::task::spawn(async move {
            http::serve(state).await;
        });
    }

    // Wait for a shutdown signal (SIGINT|SIGTERM), then gracefully shut down.
    // See `signal` module for more details.
    signal::SHUTDOWN.wait().await;
    println!("Bye");
}
