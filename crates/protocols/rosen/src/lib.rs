//! Rosen Bridge Cross-Chain Bridging Protocol
//!
//! Enables bridging ERG and tokens from Ergo to other chains (Cardano, Bitcoin,
//! Ethereum, Dogecoin, Binance, Bitcoin Runes) by creating lock transactions.
//! The Rosen Bridge watchers and guards handle the cross-chain settlement.
//!
//! A lock transaction is a pay-to-address with metadata in R4 (`Coll[Coll[SByte]]`).

pub mod config;
pub mod constants;
pub mod fee;
pub mod state;
pub mod token_map;
pub mod tx_builder;
pub mod validate;

pub use config::RosenConfig;
pub use fee::{fetch_bridge_fees, BridgeFee};
pub use state::{BridgeFeeInfo, BridgeTokenInfo, RosenBridgeState};
pub use token_map::{BridgeToken, ChainToken, TokenMap};
pub use tx_builder::{build_lock_tx, LockBuildResult, LockRequest, LockSummary};
pub use validate::validate_target_address;
