use std::fmt::Write;

use hyper::service::service_fn;
use hyper::Body;
use hyper::{server::conn::Http, Response};
use tokio::net::{TcpListener, TcpSocket};

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
    // let guard = state.pool.iter();
    // let iter = guard.iter();
    // writeln!(string, "srt_connections_active {}", iter.len()).unwrap();

    // for conn in iter {
    //     let metrics = &conn.metrics;
    //     let id = conn.id.socket_id.0;
    //     writeln!(
    //         string,
    //         "srt_connection_packets_sent{{id=\"{}\"}} {}",
    //         id, metrics.packets_sent
    //     )
    //     .unwrap();

    //     writeln!(
    //         string,
    //         "srt_connection_bytes_sent{{id=\"{}\"}} {}",
    //         id, metrics.bytes_sent
    //     )
    //     .unwrap();

    //     writeln!(
    //         string,
    //         "srt_connection_packets_recv{{id=\"{}\"}} {}",
    //         id, metrics.packets_recv
    //     )
    //     .unwrap();

    //     writeln!(
    //         string,
    //         "srt_connection_bytes_recv{{id=\"{}\"}} {}",
    //         id, metrics.bytes_recv
    //     )
    //     .unwrap();

    //     writeln!(
    //         string,
    //         "srt_connection_packets_lost{{id=\"{}\"}} {}",
    //         id, metrics.packets_dropped
    //     )
    //     .unwrap();
    // }

    Response::builder()
        .status(200)
        .body(Body::from(string))
        .unwrap()
}
