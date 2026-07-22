//! Direct swap tx builder -- spends pool box directly (no proxy/bot).
//!
//! N2T: inputs[pool, user...] -> outputs[pool', user_out, fee]
//! T2T: same structure, but ERG stays unchanged (storage rent only)
//!
//! Pool contract validates: same ErgoTree, same R4, same NFT/LP,
//! updated reserves, constant product invariant.

mod n2t;
mod t2t;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::state::{AmmError, AmmPool, PoolType, SwapInput};
use ergo_tx::{Eip12InputBox, Eip12UnsignedTx};

use self::n2t::build_n2t_direct_swap;
use self::t2t::build_t2t_direct_swap;

pub(crate) const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
pub(crate) const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

#[derive(Debug)]
pub struct DirectSwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: DirectSwapSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectSwapSummary {
    pub input_amount: u64,
    pub input_token: String,
    pub output_amount: u64,
    pub min_output: u64,
    pub output_token: String,
    pub miner_fee: u64,
    pub citadel_fee_nano: u64,
    pub total_erg_cost: u64,
}

/// Pool box must be inputs[0], new pool box must be outputs[0].
#[allow(clippy::too_many_arguments)]
pub fn build_direct_swap_eip12(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    input: &SwapInput,
    min_output: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    recipient_ergo_tree: Option<&str>,
    // Miner fee in nanoERG. `None` uses the network default (`TX_FEE`).
    // A custom fee must be at least the network minimum (1_000_000 nano).
    miner_fee_nano: Option<u64>,
) -> Result<DirectSwapBuildResult, AmmError> {
    let miner_fee = resolve_miner_fee(miner_fee_nano)?;
    match pool.pool_type {
        PoolType::N2T => build_n2t_direct_swap(
            pool_box,
            pool,
            input,
            min_output,
            user_utxos,
            user_ergo_tree,
            current_height,
            recipient_ergo_tree,
            miner_fee,
        ),
        PoolType::T2T => build_t2t_direct_swap(
            pool_box,
            pool,
            input,
            min_output,
            user_utxos,
            user_ergo_tree,
            current_height,
            recipient_ergo_tree,
            miner_fee,
        ),
    }
}

/// Minimum a fee output can be (Ergo protocol's per-byte rule effectively
/// puts the floor at ≥ 1_000_000 nano for any output with the miner-fee tree).
const MIN_FEE_NANO: u64 = 1_000_000;

fn resolve_miner_fee(custom: Option<u64>) -> Result<u64, AmmError> {
    match custom {
        None => Ok(TX_FEE),
        Some(v) if v < MIN_FEE_NANO => Err(AmmError::TxBuildError(format!(
            "Miner fee {} nano is below the network minimum {} nano",
            v, MIN_FEE_NANO
        ))),
        Some(v) => Ok(v),
    }
}
