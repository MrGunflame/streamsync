mod metrics;
mod v1;

use hyper::header::AUTHORIZATION;
use hyper::service::service_fn;
use hyper::{server::conn::Http, Response};
use hyper::{Body, Request};
use tokio::net::TcpListener;

use crate::state::State;

pub async fn serve(state: State) {
    let socket = TcpListener::bind("0.0.0.0:9998").await.unwrap();

    loop {
        let (stream, _) = socket.accept().await.unwrap();

        let state = state.clone();
        tokio::task::spawn(async move {
            let service = service_fn(move |req| {
                let mut ctx = Context {
                    state: state.clone(),
                    path: Path::new(req.uri().path()),
                    request: req,
                };

                async move {
                    let resp = match ctx.path.take() {
                        Some(path) if path == "v1" => v1::route(ctx).await,
                        Some(path) if path == "metrics" => metrics::metrics(ctx).await,
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

struct Context {
    pub request: Request<Body>,
    path: Path,
    state: State,
}

impl Context {
    pub fn authorization(&self) -> Option<&[u8]> {
        match self.request.headers().get(AUTHORIZATION) {
            Some(token) => token.as_bytes().strip_prefix(b"Bearer "),
            None => None,
        }
    }
}

struct Path {
    buf: Vec<String>,
}

impl Path {
    fn new<T>(path: T) -> Self
    where
        T: ToString,
    {
        let path = path.to_string();
        let mut buf: Vec<String> = path.split('/').map(|s| s.to_owned()).collect();

        if path.starts_with('/') {
            buf.remove(0);
        }

        Self { buf }
    }

    pub fn take(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            None
        } else {
            Some(self.buf.remove(0))
        }
    }
}
