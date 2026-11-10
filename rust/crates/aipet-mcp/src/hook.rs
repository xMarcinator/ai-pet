//! `POST /hook/claude`: Claude's HTTP hooks. It takes no token (owner's decision): the guard's Host, Origin and size
//! checks are all it has. Every other answer is 200 `{}`, at once.
//!
//! For now the body is read (so a long one without a Content-Length still gets 413) and dropped; fnt-71 task 3 turns
//! it into an envelope for the core's [`Ingress`](aipet_core::server::Ingress).

use std::sync::Arc;

use aipet_ipc::protocol::MAX_REQUEST;
use axum::Router;
use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::post;

use crate::State;
use crate::guard::empty;

pub(crate) fn routes() -> Router<Arc<State>> {
    Router::new().route("/hook/claude", post(claude))
}

async fn claude(body: Body) -> Response {
    if axum::body::to_bytes(body, MAX_REQUEST).await.is_err() {
        return empty(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let mut answer = Response::new(Body::from("{}"));
    answer.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    answer
}
