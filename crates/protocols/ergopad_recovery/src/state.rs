use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One recoverable stake position identified on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableStake {
    /// Stake key NFT token ID (hex, 64 chars). Held in the user's wallet.
    pub stake_key_id: String,
    /// Current StakeBox box ID (hex).
    pub stake_box_id: String,
    /// StakeBox ERG value in nanoERG.
    pub stake_box_value_nano: i64,
    /// ERGOPAD held by the StakeBox, raw (2 decimals).
    pub ergopad_amount_raw: i64,
    /// Checkpoint at which this stake last compounded (R4\[0\]).
    pub checkpoint: i64,
    /// Stake start time, ms since epoch (R4\[1\]).
    pub stake_time_ms: i64,
    /// Display-formatted ERGOPAD amount, e.g. `"614.68"`.
    pub ergopad_amount_display: String,
}

/// Current live state of the v1 StakeStateBox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StakeStateSnapshot {
    pub state_box_id: String,
    pub state_box_value_nano: i64,
    /// R4\[0\]: total ERGOPAD raw across all stakes.
    pub total_staked_raw: i64,
    pub checkpoint: i64,
    pub num_stakers: i64,
    pub last_checkpoint_ts: i64,
    pub cycle_duration_ms: i64,
    /// Remaining stake-token supply in the state box.
    pub stake_token_amount: i64,
}

/// Result of a full scan: live state + all stakes the user can recover.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryScan {
    pub state: StakeStateSnapshot,
    pub stakes: Vec<RecoverableStake>,
    /// Diagnostic: total candidate token IDs passed in.
    pub candidates_checked: u64,
    /// Diagnostic: how many unspent StakeBoxes we actually examined.
    pub boxes_scanned: u64,
    /// Diagnostic: pages fetched (`boxes_scanned / page_size`, rounded up).
    pub pages_fetched: u64,
    /// True if the scan stopped at the page cap without exhausting the stake P2S.
    pub hit_page_limit: bool,
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("Stake state box not found (v1 ergopad staking may be inactive)")]
    StateBoxNotFound,

    #[error("Invalid stake state box: {0}")]
    InvalidStateBox(String),

    #[error("Invalid stake box: {0}")]
    InvalidStakeBox(String),

    #[error("No StakeBox matching stake key {0} is currently unspent")]
    StakeBoxNotFound(String),

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("Transaction build failed: {0}")]
    TxBuildError(String),

    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
}
