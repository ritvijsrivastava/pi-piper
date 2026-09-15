//! `GET /api/sessions`: HTTP snapshot of the session list, used for the
//! PWA's initial load before `/ws/control` is open (and as a fallback).
//! See `SPEC.md` §6.5.
//!
//! Scoped to the caller: owned sessions are listed only for their
//! owner, unowned sessions for everyone (see `SessionMeta::owner`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::auth;
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(login) = auth::tailscale_login(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            "not authorized: connect over Tailscale",
        )
            .into_response();
    };

    Json(state.registry.visible_to(&login)).into_response()
}
