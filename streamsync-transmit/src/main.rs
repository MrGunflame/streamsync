mod listen;
mod stream;

use std::sync::mpsc;

use clap::Parser;
use stream::LiveTransmission;
use streamsync_api::{v1, Client};

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    // ARGS
    pub input: String,

    // OPTS
    /// The remote server address.
    #[arg(long, short = 'u', default_value_t = String::from("127.0.0.1"))]
    pub host: String,
    /// The port at which the HTTP server is hosted.
    #[arg(long, default_value_t = 9998)]
    pub http_port: u16,
    #[arg(long, default_value_t = String::from("http"))]
    pub http_scheme: String,
    /// The port at which the SRT server is hosted.
    #[arg(long, default_value_t = 9999)]
    pub srt_port: u16,

    /// The hosting resource.
    #[arg(long, short = 'r')]
    pub resource: String,
    /// The authentication token for the resource.
    #[arg(long, short = 't')]
    pub token: String,

    /// Enable listening mode; In listening mode a socket is opened at the specified
    /// input address instead of transmitting it.
    #[arg(long, short = 'l')]
    pub listen: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let client = Client::new(format!(
        "{}://{}:{}",
        args.http_scheme, args.host, args.http_port
    ));
    client.authorize(args.token);

    let (tx, rx) = mpsc::channel::<()>();

    let res = v1::Session::create(&client, args.resource).await?;

    let opts = SrtOptions {
        host: args.host,
        port: args.srt_port,
        resource_id: res.resource_id,
        session_id: res.session_id,
    };

    if args.listen {
        unimplemented!()
    }

    LiveTransmission::new(&opts, &args.input).run(tx);

    let _ = rx.recv();
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SrtOptions {
    pub host: String,
    pub port: u16,
    pub resource_id: String,
    pub session_id: String,
}

impl SrtOptions {
    pub fn address(&self) -> String {
        format!(
            "srt://{}:{}?streamid=#!::m=publish,r={},s={}",
            self.host, self.port, self.resource_id, self.session_id
        )
    }
}
