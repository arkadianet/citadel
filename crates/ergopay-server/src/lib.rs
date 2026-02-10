//! ErgoPay server for wallet integration
//!
//! Provides HTTP endpoints for ErgoPay protocol (EIP-0020).

pub mod handlers;
pub mod nautilus_page;
pub mod server;
pub mod types;

pub use server::ErgoPayServer;
pub use types::*;
