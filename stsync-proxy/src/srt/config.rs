use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub mtu: u32,
    pub flow_window: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mtu: 1500,
            flow_window: 8192,
        }
    }
}
