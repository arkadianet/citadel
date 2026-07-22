use citadel_core::{ProtocolError, TxError};

use crate::constants::DexyVariant;
use crate::fetch::DexyTxContext;
use crate::state::DexyState;

/// Max delay buffer for tx confirmation.
/// Must match contract's T_buffer exactly (5 blocks).
/// The contract validates: successorR4 >= HEIGHT + T_free && successorR4 <= HEIGHT + T_free + T_buffer
pub(crate) const T_BUFFER: i32 = 5;

pub fn validate_mint_dexy(amount: i64, state: &DexyState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    if !state.can_mint {
        return Err(ProtocolError::ActionNotAllowed {
            reason: "Minting not available: rate condition not met or bank empty".to_string(),
        });
    }

    if amount > state.dexy_in_bank {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds available tokens {} in bank",
                amount, state.dexy_in_bank
            ),
        });
    }

    Ok(())
}

/// Pre-flight validation matching freemint.es contract conditions.
pub fn validate_free_mint_preflight(
    ctx: &DexyTxContext,
    amount: i64,
    current_height: i32,
    variant: DexyVariant,
) -> Result<(), TxError> {
    let oracle_rate = ctx.oracle_rate_nano / variant.oracle_divisor();
    if oracle_rate <= 0 {
        return Err(TxError::BuildFailed {
            message: format!(
                "Invalid oracle rate: {} (raw: {} / divisor: {}). Oracle may be stale or unavailable.",
                oracle_rate, ctx.oracle_rate_nano, variant.oracle_divisor()
            ),
        });
    }

    let lp_rate = if ctx.lp_dexy_reserves > 0 {
        ctx.lp_erg_reserves / ctx.lp_dexy_reserves
    } else {
        return Err(TxError::BuildFailed {
            message: "LP has no Dexy reserves".to_string(),
        });
    };

    // validRateFreeMint: lpRate * 100 > oracleRate * 98
    let lp_rate_scaled = lp_rate * 100;
    let oracle_threshold = oracle_rate * 98;

    if lp_rate_scaled <= oracle_threshold {
        let lp_pct_of_oracle = (lp_rate as f64 / oracle_rate as f64) * 100.0;
        return Err(TxError::BuildFailed {
            message: format!(
                "FreeMint rate condition not met: LP rate ({} nanoERG/token, {:.2}% of oracle) must be > 98% of oracle rate ({} nanoERG/token). \
                Wait for LP/oracle prices to converge or use LP swap instead.",
                lp_rate, lp_pct_of_oracle, oracle_rate
            ),
        });
    }

    // Conservative counter reset: treat as reset if (height + T_BUFFER) > R4
    let is_counter_reset = current_height > ctx.free_mint_r4_height - T_BUFFER;
    let max_allowed_if_reset = ctx.lp_dexy_reserves / 100;
    let available_to_mint = if is_counter_reset {
        max_allowed_if_reset
    } else {
        ctx.free_mint_r5_available
    };

    if amount > available_to_mint {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds FreeMint limit {} for this period. \
                {} to mint more.",
                amount,
                available_to_mint,
                if is_counter_reset {
                    "Wait for LP reserves to increase"
                } else {
                    "Wait for FreeMint counter to reset"
                }
            ),
        });
    }

    if amount > ctx.dexy_in_bank {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds bank reserves {}",
                amount, ctx.dexy_in_bank
            ),
        });
    }

    Ok(())
}
