//! MewLock protocol state types

use serde::{Deserialize, Serialize};

/// A locked token within a MewLock box
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedToken {
    pub token_id: String,
    pub amount: u64,
    pub name: Option<String>,
    pub decimals: Option<u8>,
}

/// A single MewLock timelock box
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MewLockBox {
    /// Box ID of the lock UTXO
    pub box_id: String,
    /// Depositor's P2PK address (derived from R4 GroupElement)
    pub depositor_address: String,
    /// R5: Block height at which the lock can be withdrawn
    pub unlock_height: i32,
    /// R6: Optional timestamp (epoch seconds)
    pub timestamp: Option<i64>,
    /// R7: Optional lock name (UTF-8 string)
    pub lock_name: Option<String>,
    /// R8: Optional lock description (UTF-8 string)
    pub lock_description: Option<String>,
    /// ERG value in nanoERG
    pub erg_value: u64,
    /// Locked tokens
    pub tokens: Vec<LockedToken>,
    /// Transaction ID (for EIP-12 input)
    pub transaction_id: String,
    /// Output index in that transaction
    pub output_index: u16,
    /// Block height at which the box was created
    pub creation_height: i32,
    /// Whether this lock belongs to the connected wallet
    pub is_own: bool,
    /// Whether the lock is past unlock height
    pub is_unlockable: bool,
    /// Blocks remaining until unlock (negative = past due)
    pub blocks_remaining: i32,
}

/// Complete MewLock state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MewLockState {
    pub locks: Vec<MewLockBox>,
    pub current_height: u32,
    pub total_locks: usize,
    pub own_locks: usize,
}
