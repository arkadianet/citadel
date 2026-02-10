//! Error types for Citadel

use thiserror::Error;

/// Core errors that can occur in Citadel
#[derive(Debug, Error)]
pub enum Error {
    #[error("Node error: {0}")]
    Node(#[from] NodeError),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] TxError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Node connection and query errors
#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Node unreachable at {url}")]
    Unreachable { url: String },

    #[error("Node returned error: {message}")]
    ApiError { message: String },

    #[error("Feature requires extraIndex: {feature}")]
    ExtraIndexRequired { feature: &'static str },

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("Box not found: {box_id}")]
    BoxNotFound { box_id: String },

    #[error("Node is syncing (current height: {height})")]
    NodeSyncing { height: u32 },
}

/// Protocol-specific errors
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Protocol not available on {network}")]
    NetworkNotSupported { network: String },

    #[error("Protocol state unavailable: {reason}")]
    StateUnavailable { reason: String },

    #[error("Invalid amount: {message}")]
    InvalidAmount { message: String },

    #[error("Action not allowed: {reason}")]
    ActionNotAllowed { reason: String },

    #[error("Insufficient balance: need {required}, have {available}")]
    InsufficientBalance { required: i64, available: i64 },

    #[error("Insufficient tokens ({token}): need {required}, have {available}")]
    InsufficientTokens {
        token: String,
        required: i64,
        available: i64,
    },

    #[error("Reserve ratio {ratio:.1}% does not allow {action}")]
    RatioOutOfBounds { ratio: f64, action: String },

    #[error("Failed to parse box data: {message}")]
    BoxParseError { message: String },
}

/// Transaction building errors
#[derive(Debug, Error)]
pub enum TxError {
    #[error("Invalid address: {address}")]
    InvalidAddress { address: String },

    #[error("No UTXOs provided")]
    NoUtxos,

    #[error("Failed to build transaction: {message}")]
    BuildFailed { message: String },

    #[error("Failed to serialize transaction: {message}")]
    SerializationFailed { message: String },

    #[error("Transaction submission failed: {message}")]
    SubmissionFailed { message: String },
}

/// Result type alias for Citadel operations
pub type Result<T> = std::result::Result<T, Error>;

impl ProtocolError {
    /// Get an HTTP-friendly error code
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NetworkNotSupported { .. } => "network_not_supported",
            Self::StateUnavailable { .. } => "state_unavailable",
            Self::InvalidAmount { .. } => "invalid_amount",
            Self::ActionNotAllowed { .. } => "action_not_allowed",
            Self::InsufficientBalance { .. } => "insufficient_balance",
            Self::InsufficientTokens { .. } => "insufficient_tokens",
            Self::RatioOutOfBounds { .. } => "ratio_out_of_bounds",
            Self::BoxParseError { .. } => "box_parse_error",
        }
    }

    /// Get HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Self::InvalidAmount { .. } => 400,
            Self::InsufficientBalance { .. } | Self::InsufficientTokens { .. } => 422,
            Self::ActionNotAllowed { .. } | Self::RatioOutOfBounds { .. } => 422,
            Self::NetworkNotSupported { .. } => 422,
            Self::StateUnavailable { .. } | Self::BoxParseError { .. } => 503,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_error_codes() {
        let err = ProtocolError::InvalidAmount {
            message: "test".into(),
        };
        assert_eq!(err.error_code(), "invalid_amount");
        assert_eq!(err.status_code(), 400);

        let err = ProtocolError::InsufficientBalance {
            required: 100,
            available: 50,
        };
        assert_eq!(err.error_code(), "insufficient_balance");
        assert_eq!(err.status_code(), 422);
    }
}
