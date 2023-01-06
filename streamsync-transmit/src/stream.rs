use std::process::{Command, Stdio};
use std::sync::mpsc;

use crate::SrtOptions;

pub struct LiveTransmission {
    command: Command,
}

impl LiveTransmission {
    pub fn new(opts: &SrtOptions, source: &str) -> Self {
        let addr = opts.address();

        let mut command = Command::new("ffmpeg");
        command.args(["-re", "-i", source, "-f", "mpegts", &addr]);
        // command.stdout(Stdio::null());

        Self { command }
    }

    pub fn run(self, tx: mpsc::Sender<()>) {
        let mut cmd = self.command;

        std::thread::spawn(move || {
            let mut child = cmd.spawn().expect("failed to execute ffmpeg command");
            let status = child.wait().expect("failed to execute ffmpeg command");

            if !status.success() {
                eprintln!("ffmpeg command failed to execute");
            }

            let _ = tx.send(());
        });
    }
}
