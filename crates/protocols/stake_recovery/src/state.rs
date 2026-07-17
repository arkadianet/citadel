use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One recoverable stake position identified on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableStake {
    /// Protocol this stake belongs to, e.g. `"Ergopad"` or `"EGIO"`.
    pub protocol: String,
    /// Reward-token ticker, e.g. `"ERGOPAD"` or `"EGIO"`.
    pub reward_token_name: String,
    /// Stake key NFT token ID (hex, 64 chars). Held in the user's wallet.
    pub stake_key_id: String,
    /// Current StakeBox box ID (hex).
    pub stake_box_id: String,
    /// StakeBox ERG value in nanoERG.
    pub stake_box_value_nano: i64,
    /// Reward token held by the StakeBox, raw (protocol decimals).
    pub reward_amount_raw: i64,
    /// Checkpoint at which this stake last compounded (R4\[0\]).
    pub checkpoint: i64,
    /// Stake start time, ms since epoch (R4\[1\]).
    pub stake_time_ms: i64,
    /// Display-formatted reward amount, e.g. `"614.68"`.
    pub reward_amount_display: String,
}

/// Current live state of a v1 StakeStateBox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StakeStateSnapshot {
    /// Protocol this state box belongs to.
    pub protocol: String,
    pub state_box_id: String,
    pub state_box_value_nano: i64,
    /// R4\[0\]: total staked reward-token raw across all stakes.
    pub total_staked_raw: i64,
    pub checkpoint: i64,
    pub num_stakers: i64,
    pub last_checkpoint_ts: i64,
    pub cycle_duration_ms: i64,
    /// Remaining stake-token supply in the state box.
    pub stake_token_amount: i64,
}

/// Result of a full scan across all registered protocols: live states + every stake
/// the user can recover.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryScan {
    /// Live state snapshot per protocol that was reachable during the scan.
    pub states: Vec<StakeStateSnapshot>,
    pub stakes: Vec<RecoverableStake>,
    /// Diagnostic: total candidate token IDs passed in.
    pub candidates_checked: u64,
    /// Diagnostic: how many unspent StakeBoxes we actually examined (all protocols).
    pub boxes_scanned: u64,
    /// Diagnostic: pages fetched across all protocols.
    pub pages_fetched: u64,
    /// True if any protocol scan stopped at the page cap without exhausting its P2S.
    pub hit_page_limit: bool,
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("Stake state box not found for {0} (v1 staking may be inactive)")]
    StateBoxNotFound(String),

    #[error("Invalid stake state box: {0}")]
    InvalidStateBox(String),

    #[error("Invalid stake box: {0}")]
    InvalidStakeBox(String),

    #[error("No StakeBox matching stake key {0} is currently unspent on any registered protocol")]
    StakeBoxNotFound(String),

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("Transaction build failed: {0}")]
    TxBuildError(String),

    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
}
