use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::session::ResourceId;

#[derive(Debug)]
pub struct Database {
    pub streams: HashMap<ResourceId, Stream>,
}

impl Database {
    pub fn new() -> Self {
        let mut file = File::open("config.json").unwrap();

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();

        let mut streams = HashMap::new();

        for stream in serde_json::from_slice::<Vec<Stream>>(&buf).unwrap() {
            let id = stream.id.parse().unwrap();
            streams.insert(id, stream);
        }

        Self { streams }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stream {
    pub id: String,
    pub name: String,
    pub token: String,
}
