//! Duckpools Lending Protocol Implementation
//!
//! This crate implements the Duckpools lending protocol for Ergo.
//!
//! # Protocol Overview
//!
//! Duckpools is a decentralized lending protocol with 8 markets:
//! - ERG, SigUSD, SigRSV, RSN, rsADA, SPF, rsBTC, QUACKS
//!
//! # Architecture
//!
//! Uses proxy box pattern - users create proxy boxes that off-chain bots process.

pub mod calculator;
pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

// Re-exports
pub use calculator::*;
pub use fetch::*;
pub use state::*;
