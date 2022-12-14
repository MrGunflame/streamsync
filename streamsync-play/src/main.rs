use std::process::Command;

use streamsync_api::v1::Session;
use streamsync_api::Client;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);

    let id = args.next().unwrap();
    let token = args.next().unwrap();

    let client = Client::new("http://127.0.0.1:9998");
    client.authorize(token);
    let session = Session::create(&client, id).await.unwrap();

    ffplay(&session.resource_id, &session.session_id);
}

pub fn ffplay(r: &str, s: &str) {
    let addr = format!(
        "srt://127.0.0.1:9999?streamid=#!::m=request,r={},s={}",
        r, s
    );

    let cmd = Command::new("ffplay")
        .args(["-fflags", "nobuffer"])
        .args(["-flags", "low_delay"])
        .arg(addr)
        .spawn()
        .unwrap();
}
