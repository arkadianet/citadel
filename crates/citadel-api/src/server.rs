//! HTTP server setup and configuration

use std::net::SocketAddr;

use axum::Router;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::routes::create_router;
use crate::AppState;

/// Create the full application router with middleware
pub fn create_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    create_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

/// Start the HTTP server
pub async fn start_server(state: AppState, port: u16) -> Result<(), std::io::Error> {
    let app = create_app(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    tracing::info!("Starting API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
