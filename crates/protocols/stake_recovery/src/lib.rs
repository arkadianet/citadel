//! Stake Recovery (v1 Paideia-template staking)
//!
//! Permissionless recovery from legacy v1 staking contracts built on the shared
//! Paideia staking template (Ergopad, EGIO, …). Anyone holding a `Stake Key` NFT
//! can combine it with the live StakeStateBox and the matching StakeBox to redeem
//! the underlying reward tokens, without any action from the (now-defunct) operators.
//!
//! The StakeBox contract only verifies that some input in the tx carries the token
//! whose ID equals its R5 (the stake key). The key does not need to be burned or
//! moved — it flows through the change output back to the signer.
//!
//! Registered protocols live in [`constants::PROTOCOLS`]. Every entry shares the
//! same StakeBox / StakeStateBox script *code* (verified byte-identical across
//! Ergopad and EGIO); only the embedded token/NFT constants differ. Protocols whose
//! contract structure diverges (e.g. Paideia's own `101f`/`104e` v1 staking) are
//! deliberately not registered — see `constants.rs`.

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
