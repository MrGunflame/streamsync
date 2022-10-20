mod buffer;
mod player;
mod size;
mod source;

use std::{io::Read, sync::Arc};

use crossbeam::{channel::*, sync::WaitGroup};
use serde::{Deserialize, Serialize};

use crate::{
    buffer::VideoBuffer,
    source::{PlaybackPipeline, StreamPipeline},
};

const PATH: &str = "/home/robert/Documents/Music/Stellaris/ridingthesolarwind.ogg";
const PATH2: &str = "/home/robert/Videos/kiss.mp4";

fn main() {
    pretty_env_logger::init();
    gstreamer::init().unwrap();

    let mut file = std::fs::File::open("config.toml").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    drop(file);

    let config: Config = toml::from_slice(&buf).unwrap();

    let wg = WaitGroup::new();

    for source in config.sources {
        tracing::info!("Attaching {}", source.name);
        let wg = wg.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                let buffer = Arc::new(VideoBuffer::new());
                let stream = StreamPipeline::new(buffer.clone(), source);
                let playback = PlaybackPipeline::new(buffer);

                tokio::join! {
                    stream.run(),
                    playback.run(),
                };
            });

            drop(wg);
        });
    }

    wg.wait();
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    #[serde(default = "name_default")]
    pub name: String,
    pub location: String,
}

fn name_default() -> String {
    String::from("<unnamed>")
}
