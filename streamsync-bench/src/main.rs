use std::process::Command;

use clap::Parser;

use streamsync_api::v1::Session;
use streamsync_api::Client;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let client = Client::new(format!("http://{}:9998", args.host));
    client.authorize("test".to_owned());

    for i in 1..=args.number {
        let res = Session::create(&client, i.to_string()).await.unwrap();

        let opts = Options {
            host: args.host.clone(),
            resource_id: res.resource_id,
            session_id: res.session_id,
            file: args.input.clone(),
        };

        std::thread::spawn(move || {
            start_publish(&opts);
        });

        let res = Session::create(&client, i.to_string()).await.unwrap();
        let opts = Options {
            host: args.host.clone(),
            resource_id: res.resource_id,
            session_id: res.session_id,
            file: args.input.clone(),
        };

        std::thread::spawn(move || {
            start_request(&opts);
        });
    }

    loop {}
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, short = 'u')]
    host: String,
    #[arg(long, short = 't')]
    token: String,
    #[arg(long, short = 'i')]
    input: String,
    #[arg(long, short = 'n')]
    number: u8,
}

#[derive(Clone, Debug)]
struct Options {
    host: String,
    resource_id: String,
    session_id: String,
    file: String,
}

fn start_publish(opts: &Options) {
    let addr = format!(
        "srt://{}:9999?streamid=#!::m=publish,r={},s={}",
        opts.host, opts.resource_id, opts.session_id
    );

    let cmd = Command::new("ffmpeg")
        .args([
            "-re", "-i", &opts.file, "-acodec", "copy", "-vcodec", "copy", "-f", "mpegts", &addr,
        ])
        .spawn()
        .unwrap();
}

fn start_request(opts: &Options) {
    let addr = format!(
        "srt://{}:9999?streamid=#!::m=request,r={},s={}",
        opts.host, opts.resource_id, opts.session_id
    );

    let cmd = Command::new("ffplay").arg(addr).spawn().unwrap();
}
