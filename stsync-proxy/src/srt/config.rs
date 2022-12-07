use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// The tuple to bind the server to.
    pub bind: SocketAddr,
    /// The number of workers or ``
    pub workers: Option<usize>,

    /// The size of `SO_RCVBUF` in bytes.
    pub rcvbuf: usize,
    /// The size of `SO_SNDBUF` in bytes.
    pub sndbuf: usize,

    pub mtu: u32,
    pub flow_window: u32,
    pub buffer: u32,
}
