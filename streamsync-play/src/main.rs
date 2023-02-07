use std::process::Command;

use clap::Parser;
use streamsync_api::v1::Session;
use streamsync_api::Client;

#[derive(Clone, Debug, Parser)]
#[command(author, version, long_about = None)]
pub struct Args {
    #[arg(long, short = 'u', default_value_t = String::from("127.0.0.1"))]
    pub host: String,

    #[arg(long, default_value_t = 9998)]
    pub http_port: u16,
    #[arg(long, default_value_t = String::from("http"))]
    pub http_scheme: String,

    #[arg(long, default_value_t = 9999)]
    pub srt_port: u16,

    #[arg(long, short = 'r')]
    pub resource: String,
    #[arg(long, short = 't')]
    pub token: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let client = Client::new(format!(
        "{}://{}:{}",
        args.http_scheme, args.host, args.http_port
    ));
    client.authorize(args.token);

    let res = Session::create(&client, args.resource).await.unwrap();

    let opts = SrtOptions {
        host: args.host,
        port: args.srt_port,
        resource_id: res.resource_id,
        session_id: res.session_id,
    };

    ffplay(&opts);
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
            "srt://{}:{}?streamid=#!::m=request,r={},s={}",
            self.host, self.port, self.resource_id, self.session_id
        )
    }
}

pub fn ffplay(opts: &SrtOptions) {
    let addr = opts.address();

    let mut cmd = Command::new("ffplay");
    cmd.args(["-fflags", "nobuffer", "-flags", "low_delay", &addr]);

    cmd.spawn().unwrap().wait().unwrap();
}
