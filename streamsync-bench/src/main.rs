use std::process::Command;

use streamsync_api::v1::Session;
use streamsync_api::Client;

#[tokio::main]
async fn main() {
    let client = Client::new("http://127.0.0.1:9998");
    client.authorize("test".to_owned());

    for i in 1..=5 {
        let res = Session::create(&client, i.to_string()).await.unwrap();

        let opts = Options {
            host: "127.0.0.1:9999".into(),
            resource_id: res.resource_id,
            session_id: res.session_id,
        };

        std::thread::spawn(move || {
            start_publish(&opts);
        });

        let res = Session::create(&client, i.to_string()).await.unwrap();
        let opts = Options {
            host: "127.0.0.1:9999".into(),
            resource_id: res.resource_id,
            session_id: res.session_id,
        };

        std::thread::spawn(move || {
            start_request(&opts);
        });
    }

    loop {}
}

#[derive(Clone, Debug)]
struct Options {
    host: String,
    resource_id: String,
    session_id: String,
}

fn start_publish(opts: &Options) {
    let addr = format!(
        "srt://{}?streamid=#!::m=publish,r={},s={}",
        opts.host, opts.resource_id, opts.session_id
    );

    let file = "test.ts";

    let cmd = Command::new("ffmpeg")
        .args([
            "-re", "-i", file, "-acodec", "copy", "-vcodec", "copy", "-f", "mpegts", &addr,
        ])
        .spawn()
        .unwrap();
}

fn start_request(opts: &Options) {
    let addr = format!(
        "srt://{}?streamid=#!::m=request,r={},s={}",
        opts.host, opts.resource_id, opts.session_id
    );

    let cmd = Command::new("ffplay").arg(addr).spawn().unwrap();
}
