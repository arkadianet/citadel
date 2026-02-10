//! MEW Timelock (MewLock) Protocol Implementation
//!
//! MewLock allows users to lock ERG and/or tokens in a smart contract
//! until a specified block height. On withdrawal, a 3% fee goes to the
//! dev treasury.

pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

pub use constants::{DEV_ADDRESS, DURATION_PRESETS};
pub use fetch::fetch_mewlock_state;
pub use state::{LockedToken, MewLockBox, MewLockState};
pub use tx_builder::{build_lock_tx, build_unlock_tx};
