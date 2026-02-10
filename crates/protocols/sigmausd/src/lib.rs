//! SigmaUSD Protocol Implementation
//!
//! This crate implements the SigmaUSD (AgeUSD) algorithmic stablecoin protocol.
//!
//! # Protocol Overview
//!
//! SigmaUSD is an over-collateralized stablecoin backed by ERG:
//! - SigUSD: Stablecoin pegged to 1 USD
//! - SigRSV: Reserve token representing equity in the protocol
//!
//! # Features
//!
//! - State parsing from bank and oracle boxes
//! - Reserve ratio and price calculations
//! - Transaction building for mint/redeem operations
//!
//! # Example
//!
//! ```ignore
//! use sigmausd::{SigmaUsdProtocol, Network};
//!
//! let protocol = SigmaUsdProtocol::new(Network::Mainnet);
//! let state = protocol.get_state(&node_client).await?;
//! println!("Reserve ratio: {:.2}%", state.reserve_ratio_pct);
//! ```

pub mod calculator;
pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

pub use calculator::*;
pub use citadel_core::BoxId;
pub use constants::*;
pub use fetch::{fetch_oracle_price, fetch_sigmausd_state, OraclePrice};
pub use state::*;
