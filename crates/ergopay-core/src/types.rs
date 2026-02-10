//! ErgoPay protocol types

use serde::{Deserialize, Serialize};

/// Message severity for ErgoPay responses
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageSeverity {
    #[default]
    None,
    Information,
    Warning,
    Error,
}

/// ErgoPay signing request response
/// Sent to wallet when it fetches a signing request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ErgoPayResponse {
    /// Base64 URL-encoded reduced transaction bytes (null for connect-only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduced_tx: Option<String>,

    /// Human-readable message to display in wallet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Severity of the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_severity: Option<MessageSeverity>,

    /// Expected signer address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,

    /// URL for wallet to POST txId after submission
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}
