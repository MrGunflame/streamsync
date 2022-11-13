use std::fmt::Write;
use std::time::{Duration, Instant};

use hyper::service::service_fn;
use hyper::{server::conn::Http, Response};
use hyper::{Body, Method, Request};
use rand::rngs::OsRng;
use rand::Rng;
use tokio::net::TcpListener;

use crate::session::buffer::{BufferSessionManager, SessionKey};
use crate::session::{ResourceId, SessionId, SessionManager};
use crate::srt::state::State;

pub async fn serve(state: State<BufferSessionManager>) {
    let socket = TcpListener::bind("0.0.0.0:9998").await.unwrap();

    loop {
        let (stream, _) = socket.accept().await.unwrap();

        let state = state.clone();
        tokio::task::spawn(async move {
            let service = service_fn(move |req| {
                let state = state.clone();
                async move {
                    let resp = match (req.uri().path(), req.method()) {
                        ("/v1/metrics", &Method::GET) => metrics(&state).await,
                        (path, method) if path.starts_with("/v1/streams") => {
                            streams(req, &state).await
                        }
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
    let guard = state.conn_metrics.lock();
    let iter = guard.iter();

    writeln!(
        string,
        "srt_connections_total {}",
        state.metrics.connections_total
    )
    .unwrap();

    for (mode, gauge) in [
        ("handshake", &state.metrics.connections_handshake_current),
        ("request", &state.metrics.connections_request_current),
        ("publish", &state.metrics.connections_publish_current),
    ] {
        writeln!(
            string,
            "srt_connections_current{{mode=\"{}\"}} {}",
            mode, gauge
        )
        .unwrap();
    }

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

        writeln!(
            string,
            "srt_connection_rtt{{id=\"{}\"}} {}",
            id, metrics.rtt
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_rtt_variance{{id=\"{}\"}} {}",
            id, metrics.rtt_variance
        )
        .unwrap();
    }

    Response::builder()
        .status(200)
        .body(Body::from(string))
        .unwrap()
}

async fn streams(req: Request<Body>, state: &State<BufferSessionManager>) -> Response<Body> {
    let path = req.uri().path();
    let path = path.strip_prefix("/v1/streams").unwrap();

    let mut parts = path.split('/');
    parts.next().unwrap();

    let id: u64 = match parts.next() {
        Some(id) => match id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Response::builder()
                    .status(400)
                    .body(Body::from("Bad Request"))
                    .unwrap()
            }
        },
        None => {
            return Response::builder()
                .status(405)
                .body(Body::from("Method not allowed"))
                .unwrap()
        }
    };

    match parts.next() {
        Some("sessions") => (),
        _ => {
            return Response::builder()
                .status(404)
                .body(Body::from("Not Found"))
                .unwrap()
        }
    }

    if req.method() != Method::POST {
        return Response::builder()
            .status(405)
            .body(Body::from("Method not allowed"))
            .unwrap();
    }

    let expires = Instant::now() + Duration::from_secs(60 * 60 * 24);
    let resource_id = ResourceId(id);
    let session_id = SessionId(OsRng.gen());

    let key = SessionKey {
        expires,
        resource_id,
        session_id,
    };

    state.session_manager.registry.insert(key);

    let body = format!(
        "{{\"resource_id\":\"{}\",\"session_id\":\"{}\"}}",
        resource_id, session_id
    );

    Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(body))
        .unwrap()
}
