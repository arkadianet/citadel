//! Stake Recovery (v1 Paideia-template staking)
//!
//! Permissionless recovery from legacy v1 staking contracts built on the Paideia
//! staking tooling (Ergopad, EGIO, and Paideia's own v1 staking). Anyone holding an
//! abandoned `Stake Key` NFT can redeem the underlying reward tokens, without any
//! action from the (now-defunct) operators — via one of two [`RecoveryMechanism`]
//! variants:
//!
//! - **Direct** (Ergopad, EGIO): the StakeBox contract only verifies that some
//!   input in the tx carries the token whose ID equals its R5 (the stake key). The
//!   key does not need to be burned or moved — it flows through the change output
//!   back to the signer. Both protocols share the same StakeBox / StakeStateBox
//!   script *code* (verified byte-identical); only the embedded token/NFT
//!   constants differ.
//! - **PaideiaProxy** (Paideia): a structurally different contract family
//!   (`101f` / `104e`) redeemed via a single-use unstake proxy box (`101b`)
//!   instead of a single direct spend. The stake key is burned on a successful
//!   unstake; a permissionless refund path is the fallback if it can't run.
//!
//! Registered protocols live in [`constants::PROTOCOLS`] — see `constants.rs` and
//! `tx_builder.rs` for the full detail on each mechanism.

pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

pub use constants::{
    protocol_by_name, RecoveryMechanism, StakeProtocolConfig, EGIO, ERGOPAD, PAIDEIA,
    PAIDEIA_INCENTIVE_ERGO_TREE, PAIDEIA_PROXY_ADDRESS, PAIDEIA_PROXY_ERGO_TREE,
    PAIDEIA_PROXY_VALUE, PROTOCOLS,
};
pub use fetch::{
    discover_recoverable_stakes, fetch_stake_box_by_key, fetch_stake_state, find_stake_box_by_key,
    parse_stake_box,
};
pub use state::{RecoverableStake, RecoveryError, RecoveryScan, StakeStateSnapshot};
pub use tx_builder::{
    build_paideia_executor_tx, build_paideia_proxy_tx, build_paideia_refund_tx,
    build_recovery_tx_eip12,
};
