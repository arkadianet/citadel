//! Dexy transaction builders: FreeMint, LP Swap, LP Deposit/Redeem.
//!
//! Citadel app fee (0.011 ERG) is funded from user inputs and placed after
//! protocol successors (free-mint/bank/buyback or LP/action NFT) and before miner fee.

mod lp_deposit;
mod lp_redeem;
mod mint;
mod swap;
mod validate;

#[cfg(test)]
mod tests;

pub use lp_deposit::{build_lp_deposit_tx, LpBuildResult, LpDepositRequest, LpTxSummary};
pub use lp_redeem::{build_lp_redeem_tx, LpRedeemRequest};
pub use mint::{build_mint_dexy_tx, BuildResult, MintDexyRequest, TxSummary};
pub use swap::{
    build_swap_dexy_tx, SwapBuildResult, SwapDexyRequest, SwapDirection, SwapTxSummary,
};
pub use validate::{validate_free_mint_preflight, validate_mint_dexy};

#[cfg(test)]
pub(crate) use lp_deposit::{build_action_nft_output, build_lp_pool_output};
#[cfg(test)]
pub(crate) use mint::calculate_mint_amounts;
#[cfg(test)]
pub(crate) use swap::{build_lp_swap_output, build_swap_nft_output};
