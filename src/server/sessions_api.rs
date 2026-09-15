//! `GET /api/sessions`: HTTP snapshot of the session list, used for the
//! PWA's initial load before `/ws/control` is open (and as a fallback).
//! See `SPEC.md` §6.5.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::auth;
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !auth::is_authorized_tailscale(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            "not authorized: connect over Tailscale",
        )
            .into_response();
    }

    Json(state.registry.list()).into_response()
}
