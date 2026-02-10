//! ErgoPay protocol types (EIP-0020)

use serde::Deserialize;
use std::time::Instant;

// Re-export core types from ergopay-core
pub use ergopay_core::{ErgoPayResponse, MessageSeverity};

/// Callback payload from wallet after transaction submission
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxCallback {
    pub tx_id: String,
}

/// Type of pending request
#[derive(Debug, Clone)]
pub enum RequestType {
    /// Wallet connection request - just captures address
    Connect,
    /// Transaction signing request
    SignTransaction {
        /// Sigma-serialized reduced tx bytes (for ErgoPay mobile)
        reduced_tx: Vec<u8>,
        /// Unsigned EIP-12 tx JSON (for Nautilus desktop)
        unsigned_tx: serde_json::Value,
        /// Message to display
        message: String,
    },
}

/// Status of a pending request
#[derive(Debug, Clone)]
pub enum RequestStatus {
    /// Waiting for wallet to respond
    Pending,
    /// Address received (for connect requests)
    AddressReceived(String),
    /// Transaction submitted by wallet
    TxSubmitted { tx_id: String },
    /// Request expired
    Expired,
    /// Request failed
    Failed(String),
}

/// A pending ErgoPay request
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// Unique request ID
    pub id: String,
    /// Type of request
    pub request_type: RequestType,
    /// When the request was created
    pub created_at: Instant,
    /// Current status
    pub status: RequestStatus,
}

impl PendingRequest {
    /// Create a new connect request
    pub fn new_connect(id: String) -> Self {
        Self {
            id,
            request_type: RequestType::Connect,
            created_at: Instant::now(),
            status: RequestStatus::Pending,
        }
    }

    /// Create a new transaction signing request
    pub fn new_sign_tx(
        id: String,
        reduced_tx: Vec<u8>,
        unsigned_tx: serde_json::Value,
        message: String,
    ) -> Self {
        Self {
            id,
            request_type: RequestType::SignTransaction {
                reduced_tx,
                unsigned_tx,
                message,
            },
            created_at: Instant::now(),
            status: RequestStatus::Pending,
        }
    }

    /// Check if request has expired (5 minutes)
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() > 300
    }
}
