//! Phoenix HodlCoin Protocol Implementation
//!
//! HodlCoin is a "hold coin" protocol where users mint hodlTokens by depositing
//! ERG into a bank box. The bank charges fees on burns, ensuring the price per
//! hodlToken can only increase over time.

pub mod calculator;
pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

// Re-exports
pub use calculator::{burn_amount, hodl_price, mint_amount, BurnResult};
pub use constants::HODLERG_BANK_ERGO_TREE;
pub use fetch::{discover_banks, parse_bank_box};
pub use state::{HodlBankState, HodlBurnPreview, HodlError, HodlMintPreview};
pub use tx_builder::{build_burn_tx_eip12, build_mint_tx_eip12};
