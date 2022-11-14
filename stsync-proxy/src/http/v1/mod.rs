mod streams;

use hyper::{Body, Response};

use super::Context;

pub(super) async fn route(mut ctx: Context) -> Response<Body> {
    match ctx.path.take() {
        Some(path) if path == "streams" => streams::route(ctx).await,
        _ => Response::builder().status(404).body(Body::empty()).unwrap(),
    }
}
