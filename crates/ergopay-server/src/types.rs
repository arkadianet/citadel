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
        /// Sign-only: the wallet returns the signed tx to the server instead
        /// of broadcasting it (used for 0-conf chained txs). Nautilus-only.
        sign_only: bool,
    },
}

/// Status of a pending request
#[derive(Debug, Clone)]
pub enum RequestStatus {
    /// Waiting for wallet to respond
    Pending,
    /// Address(es) received (for connect requests).
    /// `primary` is the preferred change/display address; `addresses` is the
    /// full set from the wallet (used + unused), always including `primary`.
    AddressReceived {
        primary: String,
        addresses: Vec<String>,
    },
    /// Transaction submitted by wallet
    TxSubmitted { tx_id: String },
    /// Transaction signed and returned by wallet (sign-only requests);
    /// the app is responsible for broadcasting.
    Signed { signed_tx: serde_json::Value },
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
                sign_only: false,
            },
            created_at: Instant::now(),
            status: RequestStatus::Pending,
        }
    }

    /// Create a sign-only request: the wallet signs and returns the tx to the
    /// server without broadcasting (Nautilus-only; no reduced bytes needed).
    pub fn new_sign_only(id: String, unsigned_tx: serde_json::Value, message: String) -> Self {
        Self {
            id,
            request_type: RequestType::SignTransaction {
                reduced_tx: Vec::new(),
                unsigned_tx,
                message,
                sign_only: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_only_request_captures_signed_tx() {
        let mut req = PendingRequest::new_sign_only(
            "req1".to_string(),
            serde_json::json!({"inputs": []}),
            "Arb leg 1/3".to_string(),
        );

        assert!(matches!(
            req.request_type,
            RequestType::SignTransaction {
                sign_only: true,
                ..
            }
        ));
        assert!(matches!(req.status, RequestStatus::Pending));

        let signed = serde_json::json!({"id": "abc", "inputs": [{"spendingProof": {}}]});
        req.status = RequestStatus::Signed {
            signed_tx: signed.clone(),
        };

        match &req.status {
            RequestStatus::Signed { signed_tx } => assert_eq!(signed_tx, &signed),
            other => panic!("unexpected status: {:?}", other),
        }
    }

    #[test]
    fn normal_sign_request_is_not_sign_only() {
        let req = PendingRequest::new_sign_tx(
            "req2".to_string(),
            vec![1, 2, 3],
            serde_json::json!({}),
            "msg".to_string(),
        );
        assert!(matches!(
            req.request_type,
            RequestType::SignTransaction {
                sign_only: false,
                ..
            }
        ));
    }
}
