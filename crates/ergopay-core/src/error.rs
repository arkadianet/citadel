//! Error types for ErgoPay operations

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReductionError {
    #[error("Failed to parse box ID: {0}")]
    InvalidBoxId(String),
    #[error("Failed to parse ErgoTree: {0}")]
    InvalidErgoTree(String),
    #[error("Failed to parse value: {0}")]
    InvalidValue(String),
    #[error("Failed to parse token: {0}")]
    InvalidToken(String),
    #[error("Failed to parse register: {0}")]
    InvalidRegister(String),
    #[error("Failed to create transaction: {0}")]
    TransactionError(String),
    #[error("Failed to reduce transaction: {0}")]
    ReductionFailed(String),
    #[error("Failed to serialize: {0}")]
    SerializationError(String),
    #[error("Failed to fetch state context: {0}")]
    StateContextError(String),
}
