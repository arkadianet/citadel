//! API route handlers

pub mod amm;
pub mod dexy;
pub mod health;
pub mod lending;
pub mod node;
pub mod sigmausd;

use axum::{routing::get, Router};

use crate::AppState;

/// Create the API router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/node", node::router())
        .nest("/sigmausd", sigmausd::router())
        .nest("/dexy", dexy::router())
        .nest("/lending", lending::router())
        .nest("/amm", amm::router())
        .with_state(state)
}
