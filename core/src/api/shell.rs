//! Serves the shell. Release builds (`--features embed-shell`) carry the built shell inside the
//! binary, so the machine runs one file and no Node.js (D28). Development builds point you to the
//! Vite dev server instead.

use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

#[cfg(feature = "embed-shell")]
#[derive(rust_embed::Embed)]
#[folder = "../shell/dist/"]
struct Assets;

#[cfg(feature = "embed-shell")]
pub async fn serve(uri: Uri) -> Response {
    use axum::http::header;
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return not_found();
    }
    // Unknown paths get index.html, so the single-page app can handle them.
    let (file, name) = match Assets::get(path) {
        Some(f) if !path.is_empty() => (f, path),
        _ => match Assets::get("index.html") {
            Some(f) => (f, "index.html"),
            None => return not_found(),
        },
    };
    let mime = mime_for(name);
    ([(header::CONTENT_TYPE, mime)], file.data).into_response()
}

#[cfg(feature = "embed-shell")]
fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(not(feature = "embed-shell"))]
pub async fn serve(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return not_found();
    }
    Html(
        "<!doctype html><meta charset=utf-8><title>Faber core</title>\
         <body style=\"font-family:system-ui;max-width:40rem;margin:4rem auto\">\
         <h1>The Faber core is running</h1>\
         <p>This is a development build, so the shell isn't built in. Start it with \
         <code>npm --prefix shell run dev</code> and open the address it prints.</p>",
    )
    .into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}
