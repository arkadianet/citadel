//! Health check endpoint

use axum::Json;

use crate::dto::HealthResponse;

/// GET /health - Check API health
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}
