use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::srt;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub srt: Srt,
    pub http: Http,
}

impl Config {
    pub fn from_file<P>(path: P) -> Result<Self, Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        let mut file = File::open(path)?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        Ok(toml::from_slice(&buf)?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Http {
    pub enabled: bool,
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Srt {
    pub enabled: bool,
    pub bind: SocketAddr,
    pub workers: Option<usize>,

    pub rcvbuf: usize,
    pub sndbuf: usize,

    pub mtu: u32,
    #[serde(rename = "flow-window")]
    pub flow_window: u32,
    pub buffer: u32,
    pub latency: u16,
}

impl From<Srt> for srt::Config {
    fn from(src: Srt) -> Self {
        Self {
            workers: src.workers,
            mtu: src.mtu,
            flow_window: src.flow_window,
            bind: src.bind,
            buffer: src.buffer,
            rcvbuf: src.rcvbuf,
            sndbuf: src.sndbuf,
            latency: src.latency,
        }
    }
}
