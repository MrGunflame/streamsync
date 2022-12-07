use std::time::{Duration, Instant};

use hyper::{Body, Method, Response};
use rand::rngs::OsRng;
use rand::Rng;

use crate::http::Context;
use crate::session::buffer::SessionKey;
use crate::session::{ResourceId, SessionId};

pub(super) async fn route(mut ctx: Context, id: ResourceId) -> Response<Body> {
    match ctx.path.take() {
        Some(path) => match path.parse::<SessionId>() {
            Ok(sid) => match ctx.request.method() {
                &Method::GET => get(ctx, id, sid).await,
                &Method::DELETE => delete(ctx, id, sid).await,
                _ => Response::builder().status(405).body(Body::empty()).unwrap(),
            },
            Err(_) => Response::builder()
                .status(400)
                .body(Body::from("Failed to parse session id"))
                .unwrap(),
        },
        None => match ctx.request.method() {
            &Method::GET => list(ctx, id).await,
            &Method::POST => create(ctx, id).await,
            _ => Response::builder().status(405).body(Body::empty()).unwrap(),
        },
    }
}

async fn list(ctx: Context, id: ResourceId) -> Response<Body> {
    Response::builder().status(501).body(Body::empty()).unwrap()
}

async fn create(ctx: Context, id: ResourceId) -> Response<Body> {
    let stream = match ctx.state.db.streams.get(&id) {
        Some(stream) => stream,
        None => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    if !match ctx.authorization() {
        Some(token) => token == stream.token.as_bytes(),
        None => false,
    } {
        return Response::builder().status(401).body(Body::empty()).unwrap();
    }

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
        .status(201)
        .body(Body::from(body))
        .unwrap()
}

async fn get(ctx: Context, id: ResourceId, session_id: SessionId) -> Response<Body> {
    Response::builder().status(501).body(Body::empty()).unwrap()
}

async fn delete(ctx: Context, id: ResourceId, session_id: SessionId) -> Response<Body> {
    let stream = match ctx.state.db.streams.get(&id) {
        Some(stream) => stream,
        None => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    if !match ctx.authorization() {
        Some(token) => token == stream.token.as_bytes(),
        None => false,
    } {
        return Response::builder().status(401).body(Body::empty()).unwrap();
    }

    Response::builder().status(501).body(Body::empty()).unwrap()
}
