use std::fmt::Write;

use hyper::service::service_fn;
use hyper::Body;
use hyper::{server::conn::Http, Response};
use tokio::net::TcpListener;

use crate::session::SessionManager;
use crate::srt::state::State;

pub async fn serve<S>(state: State<S>)
where
    S: SessionManager,
{
    let socket = TcpListener::bind("0.0.0.0:9998").await.unwrap();

    loop {
        let (stream, _) = socket.accept().await.unwrap();

        let state = state.clone();
        tokio::task::spawn(async move {
            let service = service_fn(move |req| {
                let state = state.clone();
                async move {
                    let resp = match req.uri().path() {
                        "/v1/metrics" => metrics(&state).await,
                        _ => Response::builder()
                            .status(404)
                            .body(Body::from("Not Found"))
                            .unwrap(),
                    };

                    Ok::<_, hyper::Error>(resp)
                }
            });

            let conn = Http::new().serve_connection(stream, service);

            conn.await.unwrap();
        });
    }
}

async fn metrics<S: SessionManager>(state: &State<S>) -> Response<Body> {
    let mut string = String::new();
    let guard = state.metrics.lock();
    let iter = guard.iter();
    writeln!(string, "srt_connections_active {}", iter.len()).unwrap();

    for (id, metrics) in iter {
        let id = id.server_socket_id.0;

        writeln!(
            string,
            "srt_connection_data_packets_sent{{id=\"{}\"}} {}",
            id, metrics.data_packets_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_bytes_sent{{id=\"{}\"}} {}",
            id, metrics.data_bytes_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_packets_recv{{id=\"{}\"}} {}",
            id, metrics.data_packets_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_bytes_recv{{id=\"{}\"}} {}",
            id, metrics.data_bytes_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_packets_lost{{id=\"{}\"}} {}",
            id, metrics.data_packets_lost
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_bytes_lost{{id=\"{}\"}} {}",
            id, metrics.data_bytes_lost
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_packets_sent{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_sent{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_packets_recv{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_recv{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_packets_lost{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_lost
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_lost{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_lost
        )
        .unwrap();
    }

    Response::builder()
        .status(200)
        .body(Body::from(string))
        .unwrap()
}
