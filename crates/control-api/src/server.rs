//! HTTP server bootstrap.

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::response::Html;
use axum::routing::get;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::routes::{api_router, spawn_meter_pump};
use crate::state::AppState;

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Resonance</title>
<style>
body{background:#e7e2d6;color:#191b1c;font:14px/1.5 ui-monospace,monospace;margin:0;padding:48px}
h1{font-size:18px;letter-spacing:.08em;text-transform:uppercase}
code{background:#dcd6c7;padding:2px 6px;border:1px solid #c6bfad}
</style></head><body>
<h1>Resonance</h1>
<p>The control surface has not been built yet.</p>
<p>Run <code>cd web &amp;&amp; npm install &amp;&amp; npm run build</code>, then restart the daemon.</p>
<p>The API is available at <code>/api/v1/status</code>.</p>
</body></html>"#;

/// Serve the API and the built SPA.
pub async fn serve(state: AppState, addr: SocketAddr) -> anyhow::Result<()> {
    spawn_meter_pump(state.clone());

    let web_dir: PathBuf = state.inner.web_dir.clone();
    let index = web_dir.join("index.html");
    let has_web = index.is_file();

    let app = if has_web {
        let static_files = ServeDir::new(&web_dir).fallback(ServeFile::new(index));
        Router::new()
            .nest("/api/v1", api_router(state))
            .fallback_service(static_files)
    } else {
        Router::new()
            .nest("/api/v1", api_router(state))
            .fallback(get(|| async { Html(PLACEHOLDER) }))
    };

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, web = has_web, "control API listening");
    axum::serve(listener, app).await?;
    Ok(())
}
