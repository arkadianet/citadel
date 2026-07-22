//! Citadel façade: AppState, DTOs, and (Wave 2+) use-case services.
//!
//! Tauri IPC is the sole app door. ErgoPay local HTTP lives in `ergopay-server`.

pub mod dto;
pub mod services;
pub mod state;

pub use state::{ApiError, AppState, WalletState};
