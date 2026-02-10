//! Citadel-core: Shared types, errors, and configuration
//!
//! This crate provides the foundational types used across the Citadel workspace.

pub mod config;
pub mod errors;
pub mod types;

pub use config::*;
pub use errors::*;
pub use types::*;
