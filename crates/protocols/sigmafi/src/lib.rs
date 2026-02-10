//! SigmaFi P2P Bond Protocol Implementation
//!
//! SigmaFi allows borrowers to create collateralized loan requests (bond orders)
//! and lenders to fill them. Bonds have a maturity date: borrowers repay before
//! maturity or lenders can liquidate collateral after.

pub mod calculator;
pub mod constants;
pub mod fetch;
pub mod state;
pub mod tx_builder;

pub use calculator::{calculate_apr, calculate_collateral_ratio, calculate_interest_percent};
pub use constants::SUPPORTED_TOKENS;
pub use fetch::fetch_bond_market;
pub use state::{ActiveBond, BondMarket, LoanToken, OpenOrder};
pub use tx_builder::{
    build_cancel_order, build_close_order, build_liquidate, build_open_order, build_repay,
};
