//! Dexy Protocol Implementation
//!
//! This crate implements the Dexy stablecoin protocol (DexyGold and DexyUSD).
//!
//! # Protocol Overview
//!
//! Dexy is a stablecoin mechanism that maintains price stability through
//! oracle-based pricing and liquidity pool dynamics.
//!
//! - DexyGold: Pegged to gold via oracle
//! - DexyUSD (USE): Pegged to USD via oracle
//!
//! # Key Differences from SigmaUSD
//!
//! - Dexy bank is one-way (mint only) - users exit via LP swaps
//! - Uses LP dynamics + oracle arbitrage instead of reserve ratio
//!
//! # Features
//!
//! - State parsing from bank, oracle, and LP boxes
//! - Mint cost calculations at oracle rate
//! - Transaction building for mint operations

pub mod calculator;
pub mod constants;
pub mod fetch;
pub mod rates;
pub mod state;
pub mod tx_builder;

pub use calculator::*;
pub use constants::*;
pub use fetch::*;
pub use rates::DexyRates;
pub use state::*;
pub use tx_builder::*;
