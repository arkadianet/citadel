//! ErgoPay Core
//!
//! Transaction reduction and core types for ErgoPay protocol.
//!
//! This crate provides the core functionality for ErgoPay:
//! - Transaction reduction (converting EIP-12 transactions to sigma-serialized ReducedTransaction bytes)
//! - Error types for reduction operations
//!
//! # Example
//!
//! ```ignore
//! use ergopay_core::reduce_transaction;
//!
//! // After building an EIP-12 transaction with input/data-input boxes:
//! let reduced_bytes = reduce_transaction(&eip12_tx, input_boxes, data_input_boxes, &client).await?;
//! // Use reduced_bytes in ErgoPay response (base64 URL-safe encoded)
//! ```

pub mod error;
pub mod reduce;
pub mod reduce_fallback;
pub mod types;

pub use error::ReductionError;
pub use reduce::{reduce_transaction, reduce_transaction_with_context};
pub use reduce_fallback::reduce_transaction_fallback;
pub use types::{ErgoPayResponse, MessageSeverity};
