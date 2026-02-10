//! Citadel-api: HTTP API layer for Citadel
//!
//! Provides a RESTful API for the frontend to interact with the backend.

pub mod dto;
pub mod routes;
pub mod server;
pub mod state;

pub use server::*;
pub use state::{ApiError, AppState, WalletState};
