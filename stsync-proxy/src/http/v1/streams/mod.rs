use std::time::{Duration, Instant};

use hyper::header::AUTHORIZATION;
use hyper::{Body, Method, Response};
use rand::rngs::OsRng;
use rand::Rng;

use crate::http::Context;
use crate::session::buffer::SessionKey;
use crate::session::{ResourceId, SessionId};

pub(super) async fn route(mut ctx: Context) -> Response<Body> {
    match ctx.path.take() {
        Some(path) => match path.parse::<ResourceId>() {
            Ok(id) => get_stream(ctx, id).await,
            Err(_) => Response::builder()
                .status(400)
                .body(Body::from("Failed to parse id"))
                .unwrap(),
        },
        None => match ctx.request.method() {
            &Method::GET => get_streams(ctx).await,
            _ => Response::builder().status(405).body(Body::empty()).unwrap(),
        },
    }
}

async fn get_streams(_ctx: Context) -> Response<Body> {
    Response::builder().status(200).body(Body::empty()).unwrap()
}

async fn get_stream(mut ctx: Context, id: ResourceId) -> Response<Body> {
    match ctx.path.take() {
        Some(path) if path == "sessions" => (),
        // TODO: impl
        None => return Response::builder().status(404).body(Body::empty()).unwrap(),
        _ => return Response::builder().status(404).body(Body::empty()).unwrap(),
    }

    let stream = match ctx.state.db.streams.get(&id) {
        Some(stream) => stream,
        None => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    if ctx.request.method() != Method::POST {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }

    // Verify the token
    match ctx.request.headers().get(AUTHORIZATION) {
        Some(token) => match token.as_bytes().strip_prefix(b"Bearer ") {
            Some(token) => {
                if stream.token.as_bytes() != token {
                    return Response::builder().status(401).body(Body::empty()).unwrap();
                }
            }
            None => return Response::builder().status(400).body(Body::empty()).unwrap(),
        },
        None => return Response::builder().status(401).body(Body::empty()).unwrap(),
    };

    let expires = Instant::now() + Duration::from_secs(60 * 60 * 24);
    let resource_id = id;
    let session_id = SessionId(OsRng.gen());

    let key = SessionKey {
        expires,
        resource_id,
        session_id,
    };

    ctx.state.srt.session_manager.registry.insert(key);

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
