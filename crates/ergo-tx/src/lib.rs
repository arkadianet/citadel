//! ergo-tx: Transaction building utilities for Ergo
//!
//! Provides EIP-12 transaction structures and Sigma encoding utilities.

pub mod box_selector;
pub mod burn;
pub mod eip12;
pub mod sigma;
pub mod utxo_management;

#[cfg(feature = "ergo-lib")]
pub mod address;
#[cfg(feature = "ergo-lib")]
pub use address::{address_to_ergo_tree, AddressError};

#[cfg(feature = "ergo-lib")]
pub mod ergo_box_utils;

pub use box_selector::{
    collect_change_tokens, collect_multi_change_tokens, select_erg_boxes, select_multi_token_boxes,
    select_token_boxes, BoxSelectorError, SelectedInputs,
};
pub use burn::{
    build_burn_tx, build_multi_burn_tx, BurnBuildResult, BurnError, BurnItem, BurnSummary,
    MultiBurnBuildResult, MultiBurnSummary,
};
pub use eip12::*;
pub use sigma::*;
pub use utxo_management::{
    build_consolidate_tx, build_split_tx, ConsolidateBuildResult, ConsolidateSummary,
    SplitBuildResult, SplitMode, SplitSummary, UtxoManagementError,
};
