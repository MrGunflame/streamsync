use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// The size of `SO_RCVBUF` in bytes.
    pub rcvbuf: usize,
    /// The size of `SO_SNDBUF` in bytes.
    pub sndbuf: usize,

    pub mtu: u32,
    pub flow_window: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rcvbuf: 500_000,
            sndbuf: 0,
            mtu: 1500,
            flow_window: 8192,
        }
    }
}
