//! Ergopad Stake Recovery (v1)
//!
//! Permissionless recovery from the legacy v1 ergopad staking contracts.
//! Anyone holding an `ergopad Stake Key` NFT can combine it with the live
//! StakeStateBox and the matching StakeBox to redeem the underlying ERGOPAD,
//! without needing any action from the (now-defunct) Ergopad operators.
//!
//! The StakeBox contract only verifies that some input in the tx carries the
//! token whose ID equals its R5 (the stake key). The key does not need to be
//! burned or moved — it flows through the change output back to the signer.

pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

pub use fetch::{
    discover_recoverable_stakes, fetch_stake_box_by_key, fetch_stake_state, parse_stake_box,
};
pub use state::{RecoverableStake, RecoveryError, RecoveryScan, StakeStateSnapshot};
pub use tx_builder::build_recovery_tx_eip12;
