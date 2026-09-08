//! `GET /api/sessions`: HTTP snapshot of the session list, used for the
//! PWA's initial load before `/ws/control` is open (and as a fallback).
//! See `SPEC.md` §6.5.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::auth;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SessionsAuthQuery {
    token: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(query): Query<SessionsAuthQuery>,
    headers: HeaderMap,
) -> Response {
    let header_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let presented = query
        .token
        .as_deref()
        .or_else(|| auth::extract_bearer(header_token));

    if !auth::is_authorized(&state.phone_token, presented) {
        return (StatusCode::UNAUTHORIZED, "missing or invalid token").into_response();
    }

    Json(state.registry.list()).into_response()
}
