use std::net::SocketAddr;
use std::thread::available_parallelism;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// The tuple to bind the server to.
    pub bind: SocketAddr,
    /// The number of workers or ``
    pub workers: Workers,

    /// The size of `SO_RCVBUF` in bytes.
    pub rcvbuf: usize,
    /// The size of `SO_SNDBUF` in bytes.
    pub sndbuf: usize,

    pub mtu: u32,
    pub flow_window: u32,
    pub buffer: u32,

    /// Latency in millis
    pub latency: u16,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Workers(Option<usize>);

impl Workers {
    pub fn get(self) -> usize {
        self.0
            .unwrap_or_else(|| available_parallelism().map(|n| n.get()).unwrap_or(1))
    }
}
