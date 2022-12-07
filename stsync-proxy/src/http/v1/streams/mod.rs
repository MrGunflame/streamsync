mod sessions;

use hyper::{Body, Method, Response};

use crate::http::Context;
use crate::session::ResourceId;

pub(super) async fn route(mut ctx: Context) -> Response<Body> {
    match ctx.path.take() {
        Some(path) => match path.parse::<ResourceId>() {
            Ok(id) => match ctx.path.take() {
                Some(p) if p == "sessions" => sessions::route(ctx, id).await,
                _ => Response::builder().status(404).body(Body::empty()).unwrap(),
            },
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
    Response::builder().status(501).body(Body::empty()).unwrap()
}
