//! Serves the embedded mobile PWA (see `assets/`).
//!
//! Assets are embedded into the compiled binary via `rust-embed`, so
//! deployment is a single self-contained executable with no separate
//! static file directory to manage or copy around.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Serves a file embedded from `assets/`. The root path serves
/// `index.html`; any other path is looked up exactly, returning 404 if it
/// doesn't exist (the PWA is a single page, so there is no client-side
/// routing to fall back for).
pub async fn handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                file.data,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
