//! Dexy transaction builders: FreeMint, LP Swap, LP Deposit/Redeem.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use citadel_core::{constants, ProtocolError, TxError};
use ergo_tx::{
    append_change_output, collect_change_tokens, select_inputs_for_spend, Eip12Asset,
    Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{
    calculate_lp_deposit, calculate_lp_redeem, calculate_lp_swap_output,
    calculate_lp_swap_price_impact, can_redeem_lp, validate_lp_swap,
};
use crate::constants::{
    DexyVariant, BANK_FEE_NUM, BUYBACK_FEE_NUM, FEE_DENOM, LP_SWAP_FEE_DENOM, LP_SWAP_FEE_NUM,
};
use crate::fetch::{DexyLpTxContext, DexySwapTxContext, DexyTxContext};
use crate::state::DexyState;

/// FreeMint period length in blocks (1/2 day on mainnet)
const T_FREE: i32 = 360;
/// Max delay buffer for tx confirmation.
/// Must match contract's T_buffer exactly (5 blocks).
/// The contract validates: successorR4 >= HEIGHT + T_free && successorR4 <= HEIGHT + T_free + T_buffer
const T_BUFFER: i32 = 5;
/// Buyback action type for top-up (used during FreeMint/ArbMint)
/// Action 0 = swap (buy GORT), Action 1 = top-up (receive ERG), Action 2 = return (give GORT)
const BUYBACK_ACTION_TOPUP: i32 = 1;

#[derive(Debug, Clone)]
pub struct MintDexyRequest {
    pub variant: DexyVariant,
    pub amount: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TxSummary {
    pub action: String,
    pub erg_amount_nano: i64,
    pub token_amount: i64,
    pub token_name: String,
    pub tx_fee_nano: i64,
    pub bank_fee_nano: i64,
    pub buyback_fee_nano: i64,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: TxSummary,
}

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

/// Calculate ERG amounts for FreeMint matching contract formulas exactly
///
/// The contract uses specific integer division order that we must match:
/// - bankRate = oracleRate * (bankFeeNum + feeDenom) / feeDenom = oracleRate * 1003 / 1000
/// - buybackRate = oracleRate * buybackFeeNum / feeDenom = oracleRate * 2 / 1000
/// - validBankDelta: ergsAdded >= dexyMinted * bankRate
/// - validBuybackDelta: buybackErgsAdded >= dexyMinted * buybackRate
///
/// CRITICAL: The contract first computes the rate (with integer division), then multiplies
/// by amount. This order matters! For example:
/// - Contract: bankRate = 2319455 * 1003 / 1000 = 2326417; required = 100 * 2326417 = 232641700
/// - Wrong:    100 * 2319455 * 1003 / 1000 = 232641336500 / 1000 = 232641336 (364 short!)
///
/// Returns (bank_erg_added, buyback_fee)
fn calculate_mint_amounts(amount: i64, oracle_rate_nano: i64, variant: DexyVariant) -> (i64, i64) {
    // Apply oracle divisor to convert raw oracle value to nanoERG per token
    // - Gold: oracle gives nanoERG per kg, divide by 1,000,000 for per mg (per token)
    // - USD: oracle gives nanoERG per USD, divide by 1,000 for per 0.001 USE (per token)
    let oracle_rate = oracle_rate_nano / variant.oracle_divisor();

    // CRITICAL: Calculate rates FIRST (with integer division), THEN multiply by amount
    // This matches the contract's order of operations exactly
    //
    // Contract: bankRate = oracleRate * (bankFeeNum + feeDenom) / feeDenom
    // Contract: validBankDelta = ergsAdded >= dexyMinted * bankRate
    let bank_rate = oracle_rate * (FEE_DENOM + BANK_FEE_NUM) / FEE_DENOM;
    let bank_erg_added = amount * bank_rate;

    // Contract: buybackRate = oracleRate * buybackFeeNum / feeDenom
    // Contract: validBuybackDelta = buybackErgsAdded >= dexyMinted * buybackRate
    let buyback_rate = oracle_rate * BUYBACK_FEE_NUM / FEE_DENOM;
    let buyback_fee = amount * buyback_rate;

    (bank_erg_added, buyback_fee)
}

pub fn build_mint_dexy_tx(
    request: &MintDexyRequest,
    ctx: &DexyTxContext,
    state: &DexyState,
) -> Result<BuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    validate_free_mint_preflight(ctx, request.amount, request.current_height, request.variant)?;

    // CRITICAL: calculate_mint_amounts matches contract's integer division order exactly
    let (bank_erg_added, buyback_fee) =
        calculate_mint_amounts(request.amount, ctx.oracle_rate_nano, request.variant);

    let adjusted_rate = ctx.oracle_rate_nano / request.variant.oracle_divisor();
    let bank_fee = request.amount * adjusted_rate * BANK_FEE_NUM / FEE_DENOM;
    let total_cost =
        bank_erg_added + buyback_fee + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;

    let selected =
        select_inputs_for_spend(&request.user_inputs, total_cost as u64, None).map_err(|e| {
            TxError::BuildFailed {
                message: e.to_string(),
            }
        })?;

    if request.amount > state.dexy_in_bank {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds available tokens {} in bank",
                request.amount, state.dexy_in_bank
            ),
        });
    }

    // Conservative counter reset: treat as reset if (height + T_BUFFER) > R4
    // to avoid race condition where tx validates at a later block
    let is_counter_reset = request.current_height > ctx.free_mint_r4_height - T_BUFFER;
    let max_allowed_if_reset = ctx.lp_dexy_reserves / 100;
    let available_to_mint = if is_counter_reset {
        max_allowed_if_reset
    } else {
        ctx.free_mint_r5_available
    };

    if request.amount > available_to_mint {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds FreeMint limit {} for this period",
                request.amount, available_to_mint
            ),
        });
    }

    let mut buyback_input = ctx.buyback_input.clone();
    buyback_input.extension.insert(
        "0".to_string(),
        ergo_tx::sigma::encode_sigma_int(BUYBACK_ACTION_TOPUP),
    );

    let mut inputs = vec![ctx.free_mint_input.clone(), ctx.bank_input.clone(), buyback_input];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx.oracle_data_input.clone(), ctx.lp_data_input.clone()];

    let (new_r4, new_r5) = if is_counter_reset {
        (
            request.current_height + T_FREE + T_BUFFER,
            max_allowed_if_reset - request.amount,
        )
    } else {
        (
            ctx.free_mint_r4_height,
            ctx.free_mint_r5_available - request.amount,
        )
    };

    let new_bank_erg = ctx.bank_erg_nano + bank_erg_added;
    let new_dexy_in_bank = ctx.dexy_in_bank - request.amount;
    let new_buyback_erg = ctx.buyback_erg_nano + buyback_fee;

    let mut outputs = vec![
        build_free_mint_output(ctx, new_r4, new_r5, request.current_height),
        build_bank_output(ctx, new_bank_erg, new_dexy_in_bank, request.current_height),
        build_buyback_output(ctx, new_buyback_erg, request.current_height)?,
        Eip12Output::change(
            constants::MIN_BOX_VALUE_NANO,
            output_ergo_tree,
            vec![Eip12Asset::new(&state.dexy_token_id, request.amount)],
            request.current_height,
        ),
        Eip12Output::fee(constants::TX_FEE_NANO, request.current_height),
    ];

    let erg_used =
        (bank_erg_added + buyback_fee + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO)
            as u64;
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &[],
        &request.user_ergo_tree,
        request.current_height,
        constants::MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let action = format!("mint_dexy_{}", request.variant.as_str());
    let summary = TxSummary {
        action,
        erg_amount_nano: bank_erg_added + buyback_fee,
        token_amount: request.amount,
        token_name: request.variant.token_name().to_string(),
        tx_fee_nano: constants::TX_FEE_NANO,
        bank_fee_nano: bank_fee,
        buyback_fee_nano: buyback_fee,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

fn build_free_mint_output(ctx: &DexyTxContext, new_r4: i32, new_r5: i64, height: i32) -> Eip12Output {
    let registers = ergo_tx::sigma_registers!(
        "R4" => ergo_tx::sigma::encode_sigma_int(new_r4),
        "R5" => ergo_tx::sigma::encode_sigma_long(new_r5),
    );

    let assets: Vec<Eip12Asset> = ctx
        .free_mint_input
        .assets
        .iter()
        .map(|a| Eip12Asset::new(&a.token_id, a.amount.parse().unwrap_or(1)))
        .collect();

    Eip12Output {
        value: ctx.free_mint_erg_nano.to_string(),
        ergo_tree: ctx.free_mint_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: registers,
    }
}

/// Token order must exactly match input bank box.
fn build_bank_output(ctx: &DexyTxContext, new_erg: i64, new_dexy_tokens: i64, height: i32) -> Eip12Output {
    let assets: Vec<Eip12Asset> = ctx.bank_input.assets.iter().enumerate()
        .map(|(i, a)| Eip12Asset::new(&a.token_id, if i == 0 { 1 } else { new_dexy_tokens }))
        .collect();

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.bank_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: HashMap::new(),
    }
}

/// Buyback contract (action=1) requires R4 = SELF.id.
fn build_buyback_output(ctx: &DexyTxContext, new_erg: i64, height: i32) -> Result<Eip12Output, TxError> {
    let assets: Vec<Eip12Asset> = ctx.buyback_input.assets.iter()
        .map(|a| Eip12Asset::new(&a.token_id, a.amount.parse().unwrap_or(1)))
        .collect();

    let box_id_bytes: Vec<u8> =
        base16::decode(&ctx.buyback_input.box_id).map_err(|e| TxError::BuildFailed {
            message: format!("Invalid buyback box_id hex: {}", e),
        })?;
    let registers = ergo_tx::sigma_registers!(
        "R4" => ergo_tx::sigma::encode_sigma_coll_byte(&box_id_bytes)
    );

    Ok(Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.buyback_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: registers,
    })
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDirection {
    ErgToDexy,
    DexyToErg,
}

#[derive(Debug, Clone)]
pub struct SwapDexyRequest {
    pub variant: DexyVariant,
    pub direction: SwapDirection,
    pub input_amount: i64,
    pub min_output: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapTxSummary {
    pub direction: String,
    pub input_amount: i64,
    pub output_amount: i64,
    pub min_output: i64,
    pub price_impact_pct: f64,
    pub fee_pct: f64,
    pub miner_fee_nano: i64,
}

#[derive(Debug)]
pub struct SwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SwapTxSummary,
}

pub fn build_swap_dexy_tx(
    request: &SwapDexyRequest,
    ctx: &DexySwapTxContext,
    state: &DexyState,
) -> Result<SwapBuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    if request.input_amount <= 0 {
        return Err(TxError::BuildFailed {
            message: "Input amount must be positive".to_string(),
        });
    }

    let (output_amount, new_lp_erg, new_lp_dexy) = match request.direction {
        SwapDirection::ErgToDexy => {
            let out = calculate_lp_swap_output(
                request.input_amount,
                ctx.lp_erg_reserves,
                ctx.lp_dexy_reserves,
                LP_SWAP_FEE_NUM,
                LP_SWAP_FEE_DENOM,
            );
            let new_erg = ctx.lp_erg_reserves + request.input_amount;
            let new_dexy = ctx.lp_dexy_reserves - out;
            (out, new_erg, new_dexy)
        }
        SwapDirection::DexyToErg => {
            let out = calculate_lp_swap_output(
                request.input_amount,
                ctx.lp_dexy_reserves,
                ctx.lp_erg_reserves,
                LP_SWAP_FEE_NUM,
                LP_SWAP_FEE_DENOM,
            );
            let new_erg = ctx.lp_erg_reserves - out;
            let new_dexy = ctx.lp_dexy_reserves + request.input_amount;
            (out, new_erg, new_dexy)
        }
    };

    if output_amount <= 0 {
        return Err(TxError::BuildFailed {
            message: "Output amount is zero or negative".to_string(),
        });
    }

    if output_amount < request.min_output {
        return Err(TxError::BuildFailed {
            message: format!(
                "Output {} below minimum {}",
                output_amount, request.min_output
            ),
        });
    }

    let (delta_x, delta_y) = match request.direction {
        SwapDirection::ErgToDexy => (request.input_amount, -output_amount),
        SwapDirection::DexyToErg => (-output_amount, request.input_amount),
    };
    if !validate_lp_swap(
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        delta_x,
        delta_y,
        LP_SWAP_FEE_NUM,
        LP_SWAP_FEE_DENOM,
    ) {
        return Err(TxError::BuildFailed {
            message: "Swap fails contract validation".to_string(),
        });
    }

    let selected = match request.direction {
        SwapDirection::ErgToDexy => {
            let needed =
                request.input_amount + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
            select_inputs_for_spend(&request.user_inputs, needed as u64, None)
        }
        SwapDirection::DexyToErg => {
            let min_erg = constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
            select_inputs_for_spend(
                &request.user_inputs,
                min_erg as u64,
                Some((&state.dexy_token_id, request.input_amount as u64)),
            )
        }
    }
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let mut inputs = vec![ctx.lp_input.clone(), ctx.swap_input.clone()];
    inputs.extend(selected.boxes.clone());

    let lp_output = build_lp_swap_output(
        ctx,
        new_lp_erg,
        new_lp_dexy,
        &state.dexy_token_id,
        request.current_height,
    );

    let mut outputs = vec![lp_output, build_swap_nft_output(ctx, request.current_height)];

    match request.direction {
        SwapDirection::ErgToDexy => {
            let user_output_erg = constants::MIN_BOX_VALUE_NANO;
            outputs.push(Eip12Output::change(
                user_output_erg,
                output_ergo_tree,
                vec![Eip12Asset::new(&state.dexy_token_id, output_amount)],
                request.current_height,
            ));

            outputs.push(Eip12Output::fee(constants::TX_FEE_NANO, request.current_height));

            let erg_used = (request.input_amount + constants::TX_FEE_NANO + user_output_erg) as u64;
            append_change_output(
                &mut outputs,
                &selected,
                erg_used,
                &[],
                &request.user_ergo_tree,
                request.current_height,
                constants::MIN_BOX_VALUE_NANO as u64,
            )
            .map_err(|e| TxError::BuildFailed {
                message: e.to_string(),
            })?;
        }
        SwapDirection::DexyToErg => {
            let user_output_erg =
                selected.total_erg as i64 + output_amount - constants::TX_FEE_NANO;
            let remaining_assets = collect_change_tokens(
                &selected.boxes,
                Some((&state.dexy_token_id, request.input_amount as u64)),
            );

            outputs.push(Eip12Output::change(
                user_output_erg,
                output_ergo_tree,
                remaining_assets,
                request.current_height,
            ));

            // Miner fee
            outputs.push(Eip12Output::fee(
                constants::TX_FEE_NANO,
                request.current_height,
            ));
        }
    }

    // 11. Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // Calculate price impact for summary
    let price_impact = calculate_lp_swap_price_impact(
        request.input_amount,
        match request.direction {
            SwapDirection::ErgToDexy => ctx.lp_erg_reserves,
            SwapDirection::DexyToErg => ctx.lp_dexy_reserves,
        },
        match request.direction {
            SwapDirection::ErgToDexy => ctx.lp_dexy_reserves,
            SwapDirection::DexyToErg => ctx.lp_erg_reserves,
        },
        LP_SWAP_FEE_NUM,
        LP_SWAP_FEE_DENOM,
    );

    let summary = SwapTxSummary {
        direction: match request.direction {
            SwapDirection::ErgToDexy => "erg_to_dexy".to_string(),
            SwapDirection::DexyToErg => "dexy_to_erg".to_string(),
        },
        input_amount: request.input_amount,
        output_amount,
        min_output: request.min_output,
        price_impact_pct: price_impact,
        fee_pct: LP_SWAP_FEE_NUM as f64 / LP_SWAP_FEE_DENOM as f64 * 100.0,
        miner_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(SwapBuildResult {
        unsigned_tx,
        summary,
    })
}

/// Build updated LP box output for swap transaction
///
/// Preserves token order from the input LP box while updating the ERG value
/// and Dexy token amount to reflect the swap.
fn build_lp_swap_output(
    ctx: &DexySwapTxContext,
    new_erg: i64,
    new_dexy: i64,
    dexy_token_id: &str,
    height: i32,
) -> Eip12Output {
    // Preserve token order from input LP box, updating amounts
    let mut assets = Vec::new();
    for (token_id, amount) in &ctx.lp_tokens {
        if token_id == dexy_token_id {
            assets.push(Eip12Asset::new(token_id, new_dexy));
        } else {
            assets.push(Eip12Asset::new(token_id, *amount as i64));
        }
    }

    // Copy registers from input LP box
    let additional_registers = ctx.lp_input.additional_registers.clone();

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.lp_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers,
    }
}

/// Build preserved Swap NFT box output (exact copy)
///
/// The Swap NFT box must be reproduced exactly in the output to satisfy
/// the contract's self-preservation check.
fn build_swap_nft_output(ctx: &DexySwapTxContext, height: i32) -> Eip12Output {
    let assets: Vec<Eip12Asset> = ctx
        .swap_tokens
        .iter()
        .map(|(id, amt)| Eip12Asset::new(id, *amt as i64))
        .collect();

    Eip12Output {
        value: ctx.swap_erg_value.to_string(),
        ergo_tree: ctx.swap_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: ctx.swap_input.additional_registers.clone(),
    }
}

// =============================================================================
// LP Deposit / Redeem Transaction Builders
// =============================================================================

/// Request to build an LP deposit (add liquidity) transaction
#[derive(Debug, Clone)]
pub struct LpDepositRequest {
    pub variant: DexyVariant,
    pub deposit_erg: i64,
    pub deposit_dexy: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    pub recipient_ergo_tree: Option<String>,
}

/// Request to build an LP redeem (remove liquidity) transaction
#[derive(Debug, Clone)]
pub struct LpRedeemRequest {
    pub variant: DexyVariant,
    pub lp_to_burn: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    pub recipient_ergo_tree: Option<String>,
}

/// Summary of an LP deposit or redeem transaction for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpTxSummary {
    pub action: String,
    pub erg_amount: i64,
    pub dexy_amount: i64,
    pub lp_tokens: i64,
    pub miner_fee_nano: i64,
}

/// Build result for LP deposit/redeem transactions
#[derive(Debug)]
pub struct LpBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LpTxSummary,
}

/// Build updated LP pool box output for deposit/redeem transactions.
///
/// Preserves token order from the input LP box while updating the ERG value,
/// LP token reserve, and Dexy token amount.
fn build_lp_pool_output(
    ctx: &DexyLpTxContext,
    new_erg: i64,
    new_lp_tokens: i64,
    new_dexy: i64,
    lp_token_id: &str,
    dexy_token_id: &str,
    height: i32,
) -> Eip12Output {
    // Preserve token order from ctx.lp_tokens, update amounts
    let mut assets = Vec::new();
    for (token_id, amount) in &ctx.lp_tokens {
        if token_id == lp_token_id {
            assets.push(Eip12Asset::new(token_id, new_lp_tokens));
        } else if token_id == dexy_token_id {
            assets.push(Eip12Asset::new(token_id, new_dexy));
        } else {
            assets.push(Eip12Asset::new(token_id, *amount as i64));
        }
    }

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.lp_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: ctx.lp_input.additional_registers.clone(),
    }
}

/// Build preserved action NFT box output (exact self-preservation).
///
/// Used for both LP Mint and LP Redeem action boxes.
fn build_action_nft_output(ctx: &DexyLpTxContext, height: i32) -> Eip12Output {
    let assets: Vec<Eip12Asset> = ctx
        .action_tokens
        .iter()
        .map(|(id, amt)| Eip12Asset::new(id, *amt as i64))
        .collect();

    Eip12Output {
        value: ctx.action_erg_value.to_string(),
        ergo_tree: ctx.action_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: ctx.action_input.additional_registers.clone(),
    }
}

/// Build an LP deposit (add liquidity) transaction.
///
/// Creates an unsigned EIP-12 transaction that deposits ERG and Dexy tokens
/// into the LP pool in exchange for LP tokens.
///
/// # Transaction Structure
///
/// **Inputs:**
/// - 0: LP box (lpNFT)
/// - 1: LP Mint box (lpMintNFT) -- persistent action singleton
/// - 2+: User UTXOs (must contain deposit_erg + deposit_dexy)
///
/// **Data Inputs:** (none)
///
/// **Outputs:**
/// - 0: Updated LP box (more ERG, more Dexy, fewer reserved LP tokens)
/// - 1: LP Mint box (exact self-preservation)
/// - 2: User output (LP tokens received + change ERG + remaining tokens)
/// - 3: Miner fee
pub fn build_lp_deposit_tx(
    request: &LpDepositRequest,
    ctx: &DexyLpTxContext,
    dexy_token_id: &str,
    lp_token_id: &str,
    initial_lp: i64,
) -> Result<LpBuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // 1. Validate deposit amounts
    if request.deposit_erg <= 0 {
        return Err(TxError::BuildFailed {
            message: "Deposit ERG amount must be positive".to_string(),
        });
    }
    if request.deposit_dexy <= 0 {
        return Err(TxError::BuildFailed {
            message: "Deposit Dexy amount must be positive".to_string(),
        });
    }

    // 2. Calculate LP tokens to receive
    let calc = calculate_lp_deposit(
        request.deposit_erg,
        request.deposit_dexy,
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        ctx.lp_token_reserves,
        initial_lp,
    );

    if calc.lp_tokens_out <= 0 {
        return Err(TxError::BuildFailed {
            message: "Deposit too small: would receive 0 LP tokens".to_string(),
        });
    }

    tracing::debug!("=== LP Deposit Transaction Build ===");
    tracing::debug!(
        "Deposit: erg={}, dexy={}",
        request.deposit_erg,
        request.deposit_dexy
    );
    tracing::debug!(
        "LP reserves: erg={}, dexy={}, lp_tokens={}",
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        ctx.lp_token_reserves
    );
    tracing::debug!(
        "Calculated: lp_tokens_out={}, consumed_erg={}, consumed_dexy={}",
        calc.lp_tokens_out,
        calc.consumed_erg,
        calc.consumed_dexy
    );

    // 3. Select user UTXOs: need consumed_erg + TX_FEE + MIN_BOX_VALUE (for user output), and consumed_dexy tokens
    let min_erg = calc.consumed_erg + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected = select_inputs_for_spend(
        &request.user_inputs,
        min_erg as u64,
        Some((dexy_token_id, calc.consumed_dexy as u64)),
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let mut inputs = vec![ctx.lp_input.clone(), ctx.action_input.clone()];
    inputs.extend(selected.boxes.clone());

    let new_lp_erg = ctx.lp_erg_reserves + calc.consumed_erg;
    let new_lp_dexy = ctx.lp_dexy_reserves + calc.consumed_dexy;
    let new_lp_token_reserves = ctx.lp_token_reserves - calc.lp_tokens_out;

    let mut user_tokens = vec![Eip12Asset::new(lp_token_id, calc.lp_tokens_out)];
    user_tokens.extend(collect_change_tokens(
        &selected.boxes,
        Some((dexy_token_id, calc.consumed_dexy as u64)),
    ));

    let user_erg = selected.total_erg as i64 - calc.consumed_erg - constants::TX_FEE_NANO;
    let outputs = vec![
        build_lp_pool_output(ctx, new_lp_erg, new_lp_token_reserves, new_lp_dexy, lp_token_id, dexy_token_id, request.current_height),
        build_action_nft_output(ctx, request.current_height),
        Eip12Output::change(user_erg.max(constants::MIN_BOX_VALUE_NANO), output_ergo_tree, user_tokens, request.current_height),
        Eip12Output::fee(constants::TX_FEE_NANO, request.current_height),
    ];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let summary = LpTxSummary {
        action: format!("lp_deposit_{}", request.variant.as_str()),
        erg_amount: calc.consumed_erg,
        dexy_amount: calc.consumed_dexy,
        lp_tokens: calc.lp_tokens_out,
        miner_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(LpBuildResult {
        unsigned_tx,
        summary,
    })
}

pub fn build_lp_redeem_tx(
    request: &LpRedeemRequest,
    ctx: &DexyLpTxContext,
    dexy_token_id: &str,
    lp_token_id: &str,
    initial_lp: i64,
) -> Result<LpBuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    if request.lp_to_burn <= 0 {
        return Err(TxError::BuildFailed {
            message: "LP tokens to burn must be positive".to_string(),
        });
    }

    let oracle_rate_nano = ctx.oracle_rate_nano.ok_or_else(|| TxError::BuildFailed {
        message: "Oracle rate required for LP redeem but not available".to_string(),
    })?;

    let oracle_rate_adjusted = oracle_rate_nano / request.variant.oracle_divisor();

    if !can_redeem_lp(
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        oracle_rate_adjusted,
    ) {
        return Err(TxError::BuildFailed {
            message: "LP redeem blocked: LP rate below 98% of oracle rate (depeg protection)"
                .to_string(),
        });
    }

    let calc = calculate_lp_redeem(
        request.lp_to_burn,
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        ctx.lp_token_reserves,
        initial_lp,
    );

    if calc.erg_out <= 0 || calc.dexy_out <= 0 {
        return Err(TxError::BuildFailed {
            message: "Redeem too small: would receive 0 ERG or Dexy tokens".to_string(),
        });
    }

    let min_erg = constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected = select_inputs_for_spend(
        &request.user_inputs,
        min_erg as u64,
        Some((lp_token_id, request.lp_to_burn as u64)),
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let mut inputs = vec![ctx.lp_input.clone(), ctx.action_input.clone()];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx
        .oracle_data_input
        .as_ref()
        .ok_or_else(|| TxError::BuildFailed {
            message: "Oracle data input required for LP redeem but not available".to_string(),
        })?
        .clone()];

    let new_lp_erg = ctx.lp_erg_reserves - calc.erg_out;
    let new_lp_dexy = ctx.lp_dexy_reserves - calc.dexy_out;
    let new_lp_token_reserves = ctx.lp_token_reserves + request.lp_to_burn;

    let mut user_assets = vec![Eip12Asset::new(dexy_token_id, calc.dexy_out)];
    user_assets.extend(collect_change_tokens(
        &selected.boxes,
        Some((lp_token_id, request.lp_to_burn as u64)),
    ));

    let user_output_erg = selected.total_erg as i64 + calc.erg_out - constants::TX_FEE_NANO;
    let outputs = vec![
        build_lp_pool_output(ctx, new_lp_erg, new_lp_token_reserves, new_lp_dexy, lp_token_id, dexy_token_id, request.current_height),
        build_action_nft_output(ctx, request.current_height),
        Eip12Output::change(user_output_erg, output_ergo_tree, user_assets, request.current_height),
        Eip12Output::fee(constants::TX_FEE_NANO, request.current_height),
    ];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let summary = LpTxSummary {
        action: format!("lp_redeem_{}", request.variant.as_str()),
        erg_amount: calc.erg_out,
        dexy_amount: calc.dexy_out,
        lp_tokens: request.lp_to_burn,
        miner_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(LpBuildResult {
        unsigned_tx,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mint_positive_amount() {
        let state = create_test_state(10000, true);

        assert!(validate_mint_dexy(100, &state).is_ok());

        let result = validate_mint_dexy(0, &state);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));

        let result = validate_mint_dexy(-100, &state);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mint_can_mint() {
        let state = create_test_state(0, false);

        let result = validate_mint_dexy(100, &state);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ProtocolError::ActionNotAllowed { .. })
        ));
    }

    #[test]
    fn test_validate_mint_exceeds_available() {
        let state = create_test_state(1000, true);

        let result = validate_mint_dexy(2000, &state);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));
    }

    #[test]
    fn test_calculate_mint_amounts_use() {
        // Test with USE (oracle divisor = 1000)
        // Oracle rate: 1_850_000_000 nanoERG per USD (1.85 ERG per USD)
        // Amount: 1_000 (1 USE with 3 decimals)
        let (bank_erg_added, buyback_fee) =
            calculate_mint_amounts(1_000, 1_850_000_000, DexyVariant::Usd);

        // Adjusted rate = 1_850_000_000 / 1000 = 1_850_000
        // Contract formula (order matters!):
        //   bankRate = 1_850_000 * 1003 / 1000 = 1_855_550
        //   bank_erg_added = 1_000 * 1_855_550 = 1_855_550_000
        //   buybackRate = 1_850_000 * 2 / 1000 = 3_700
        //   buyback_fee = 1_000 * 3_700 = 3_700_000
        assert_eq!(bank_erg_added, 1_855_550_000);
        assert_eq!(buyback_fee, 3_700_000);
    }

    #[test]
    fn test_calculate_mint_amounts_gold() {
        // Test with DexyGold (oracle divisor = 1_000_000)
        // Oracle rate: 220_000_000_000 nanoERG per kg (220 ERG per kg)
        // Amount: 10 (10 DexyGold tokens = 10 mg)
        let (bank_erg_added, buyback_fee) =
            calculate_mint_amounts(10, 220_000_000_000, DexyVariant::Gold);

        // Adjusted rate = 220_000_000_000 / 1_000_000 = 220_000 nanoERG per mg
        // Contract formula (order matters!):
        //   bankRate = 220_000 * 1003 / 1000 = 220_660
        //   bank_erg_added = 10 * 220_660 = 2_206_600
        //   buybackRate = 220_000 * 2 / 1000 = 440
        //   buyback_fee = 10 * 440 = 4_400
        assert_eq!(bank_erg_added, 2_206_600);
        assert_eq!(buyback_fee, 4_400);
    }

    #[test]
    fn test_integer_division_order_matters() {
        // This test verifies our calculation matches contract's integer division exactly
        //
        // The order of operations matters due to integer division:
        // - (amount * rate * 1003) / 1000 gives different result than
        // - amount * (rate * 1003 / 1000)
        //
        // The contract uses the latter form, so we must match it.

        let amount: i64 = 100;
        let oracle_rate_nano: i64 = 2_319_455_000; // Example oracle value
        let variant = DexyVariant::Usd;

        let oracle_rate = oracle_rate_nano / variant.oracle_divisor(); // 2_319_455

        // Wrong order: (amount * rate * 1003) / 1000
        // = (100 * 2_319_455 * 1003) / 1000 = 232_641_336_500 / 1000 = 232_641_336
        let wrong_order = amount * oracle_rate * 1003 / 1000;

        // Contract's order: amount * (rate * 1003 / 1000)
        // = 100 * (2_319_455 * 1003 / 1000) = 100 * 2_326_413 = 232_641_300
        let bank_rate = oracle_rate * 1003 / 1000;
        let contract_order = amount * bank_rate;

        // Our new (correct) calculation
        let (new_bank_erg_added, _) = calculate_mint_amounts(amount, oracle_rate_nano, variant);

        // The two orders give different results (36 nanoERG difference in this case)
        assert_ne!(
            wrong_order, contract_order,
            "Different division order should give different results"
        );

        // New calculation matches contract order exactly
        assert_eq!(
            new_bank_erg_added, contract_order,
            "New calculation {} should equal contract order {}",
            new_bank_erg_added, contract_order
        );
    }

    #[test]
    fn test_tx_summary() {
        let summary = TxSummary {
            action: "mint_dexy_gold".to_string(),
            erg_amount_nano: 1_000_000_000,
            token_amount: 100,
            token_name: "DexyGold".to_string(),
            tx_fee_nano: 1_100_000,
            bank_fee_nano: 3_000_000,
            buyback_fee_nano: 2_000_000,
        };

        assert_eq!(summary.action, "mint_dexy_gold");
        assert_eq!(summary.token_name, "DexyGold");
    }

    // Helper functions for tests

    fn create_test_state(dexy_in_bank: i64, can_mint: bool) -> DexyState {
        DexyState {
            variant: DexyVariant::Gold,
            bank_erg_nano: 1_000_000_000_000,
            dexy_in_bank,
            bank_box_id: "bank_box_123".to_string(),
            dexy_token_id: "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad"
                .to_string(),
            free_mint_available: 5_000,
            free_mint_reset_height: 1_000_000,
            current_height: 999_500,
            oracle_rate_nano: 1_000_000_000,
            oracle_box_id: "oracle_box_456".to_string(),
            lp_erg_reserves: 500_000_000_000,
            lp_dexy_reserves: 500_000,
            lp_box_id: "lp_box_789".to_string(),
            lp_rate_nano: 1_000_000,
            lp_token_reserves: 0,
            lp_circulating: 0,
            can_redeem_lp: true,
            can_mint,
            rate_difference_pct: 0.0,
            dexy_circulating: 0,
        }
    }

    fn create_test_input(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "test_box".to_string(),
            transaction_id: "test_tx".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd...".to_string(),
            assets: tokens
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt))
                .collect(),
            creation_height: 12345,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    mod swap_tests {
        use super::*;
        use crate::fetch::DexySwapTxContext;

        const DEXY_TOKEN_ID: &str =
            "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
        const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
        const LP_TOKEN_ID: &str = "lp_token_id_placeholder";
        const SWAP_NFT_ID: &str =
            "ff7b7eff3c818f9dc573ca03a723a7f6ed1615bf27980ebd4a6c91986b26f801";

        fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
            use ergo_lib::ergotree_ir::chain::ergo_box::{
                box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
            };
            use ergo_lib::ergotree_ir::chain::tx_id::TxId;
            use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
            use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

            // Use a minimal P2PK ErgoTree (simplest valid tree)
            let ergo_tree_bytes = base16::decode(
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            )
            .unwrap();
            let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).unwrap();
            let tx_id = TxId::zero();

            ErgoBox::new(
                BoxValue::new(1_000_000).unwrap(),
                ergo_tree,
                None,
                NonMandatoryRegisters::empty(),
                100000,
                tx_id,
                0,
            )
            .unwrap()
        }

        fn create_test_swap_context(lp_erg: i64, lp_dexy: i64) -> DexySwapTxContext {
            let lp_input = Eip12InputBox {
                box_id: "lp_box_id".to_string(),
                transaction_id: "lp_tx_id".to_string(),
                index: 0,
                value: lp_erg.to_string(),
                ergo_tree: "lp_ergo_tree_hex".to_string(),
                assets: vec![
                    Eip12Asset::new(LP_NFT_ID, 1),
                    Eip12Asset::new(LP_TOKEN_ID, 9_000_000_000_000_000i64),
                    Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
                ],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let swap_input = Eip12InputBox {
                box_id: "swap_box_id".to_string(),
                transaction_id: "swap_tx_id".to_string(),
                index: 0,
                value: "1000000".to_string(),
                ergo_tree: "swap_ergo_tree_hex".to_string(),
                assets: vec![Eip12Asset::new(SWAP_NFT_ID, 1)],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let dummy_box = create_dummy_ergo_box();

            DexySwapTxContext {
                lp_input,
                lp_erg_reserves: lp_erg,
                lp_dexy_reserves: lp_dexy,
                lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
                lp_box: dummy_box.clone(),
                lp_tokens: vec![
                    (LP_NFT_ID.to_string(), 1),
                    (LP_TOKEN_ID.to_string(), 9_000_000_000_000_000),
                    (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
                ],
                swap_input,
                swap_erg_value: 1_000_000,
                swap_ergo_tree: "swap_ergo_tree_hex".to_string(),
                swap_box: dummy_box,
                swap_tokens: vec![(SWAP_NFT_ID.to_string(), 1)],
            }
        }

        fn create_swap_state() -> DexyState {
            create_test_state(10000, true)
        }

        fn create_erg_to_dexy_request(
            input_amount: i64,
            min_output: i64,
            user_erg: i64,
        ) -> SwapDexyRequest {
            SwapDexyRequest {
                variant: DexyVariant::Gold,
                direction: SwapDirection::ErgToDexy,
                input_amount,
                min_output,
                user_address: "user_address".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(user_erg, vec![])],
                current_height: 100000,
                recipient_ergo_tree: None,
            }
        }

        fn create_dexy_to_erg_request(
            input_amount: i64,
            min_output: i64,
            user_erg: i64,
            user_dexy: i64,
        ) -> SwapDexyRequest {
            SwapDexyRequest {
                variant: DexyVariant::Gold,
                direction: SwapDirection::DexyToErg,
                input_amount,
                min_output,
                user_address: "user_address".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    user_erg,
                    vec![(DEXY_TOKEN_ID, user_dexy)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            }
        }


        #[test]
        fn test_build_lp_swap_output_updates_dexy_amount() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

            let output = build_lp_swap_output(
                &ctx,
                1_001_000_000_000, // new ERG (added 1 ERG)
                999_000,           // new Dexy (removed 1000)
                DEXY_TOKEN_ID,
                100001,
            );

            assert_eq!(output.value, "1001000000000");
            assert_eq!(output.ergo_tree, "lp_ergo_tree_hex");
            assert_eq!(output.assets.len(), 3);

            assert_eq!(output.assets[0].token_id, LP_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");

            assert_eq!(output.assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(output.assets[1].amount, "9000000000000000");

            assert_eq!(output.assets[2].token_id, DEXY_TOKEN_ID);
            assert_eq!(output.assets[2].amount, "999000");
        }

        #[test]
        fn test_build_swap_nft_output_preserves_exactly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

            let output = build_swap_nft_output(&ctx, 100001);

            assert_eq!(output.value, "1000000");
            assert_eq!(output.ergo_tree, "swap_ergo_tree_hex");
            assert_eq!(output.assets.len(), 1);
            assert_eq!(output.assets[0].token_id, SWAP_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");
        }


        #[test]
        fn test_swap_rejects_zero_input() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(0, 1, 10_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            let err = result.unwrap_err();
            match err {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("positive"), "Got: {}", message);
                }
                _ => panic!("Expected BuildFailed, got {:?}", err),
            }
        }

        #[test]
        fn test_swap_rejects_negative_input() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(-100, 1, 10_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
        }

        #[test]
        fn test_swap_rejects_insufficient_erg() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(
                10_000_000_000, // 10 ERG input
                1,
                1_000_000_000, // only 1 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient ERG"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_swap_rejects_insufficient_dexy_tokens() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_dexy_to_erg_request(
                1000,           // sell 1000 Dexy
                1,              // min output
                10_000_000_000, // user has 10 ERG for fees
                100,            // but only 100 Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient token"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_swap_rejects_slippage_violation() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(
                1_000_000_000,   // 1 ERG
                999_999_999,     // impossibly high min output
                100_000_000_000, // 100 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("below minimum"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }


        #[test]
        fn test_erg_to_dexy_swap_builds_correctly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(
                1_000_000_000,   // swap 1 ERG
                1,               // min 1 Dexy output
                100_000_000_000, // 100 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            assert_eq!(tx.inputs.len(), 3);
            assert_eq!(tx.inputs[0].box_id, "lp_box_id");
            assert_eq!(tx.inputs[1].box_id, "swap_box_id");
            assert_eq!(tx.data_inputs.len(), 0);
            assert!(tx.outputs.len() >= 4);
            assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
            assert_eq!(tx.outputs[1].ergo_tree, "swap_ergo_tree_hex");
            assert_eq!(tx.outputs[1].value, "1000000");
            assert_eq!(tx.outputs[2].ergo_tree, "user_ergo_tree");
            assert_eq!(tx.outputs[2].assets.len(), 1);
            assert_eq!(tx.outputs[2].assets[0].token_id, DEXY_TOKEN_ID);
            let user_dexy_out: i64 = tx.outputs[2].assets[0].amount.parse().unwrap();
            assert!(user_dexy_out > 0, "User should receive Dexy tokens");
            assert_eq!(tx.outputs[3].value, constants::TX_FEE_NANO.to_string());

            assert_eq!(build.summary.direction, "erg_to_dexy");
            assert_eq!(build.summary.input_amount, 1_000_000_000);
            assert!(build.summary.output_amount > 0);
            assert_eq!(build.summary.fee_pct, 0.3);
        }

        #[test]
        fn test_dexy_to_erg_swap_builds_correctly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_dexy_to_erg_request(
                100,            // sell 100 Dexy
                1,              // min 1 nanoERG output
                10_000_000_000, // 10 ERG for fees
                1000,           // have 1000 Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            assert_eq!(tx.inputs.len(), 3);
            assert_eq!(tx.outputs.len(), 4);

            let user_output = &tx.outputs[2];
            assert_eq!(user_output.ergo_tree, "user_ergo_tree");
            let user_erg_out: i64 = user_output.value.parse().unwrap();
            assert!(
                user_erg_out > 10_000_000_000,
                "User should receive more ERG than started with"
            );

            let remaining_dexy = user_output
                .assets
                .iter()
                .find(|a| a.token_id == DEXY_TOKEN_ID);
            assert!(remaining_dexy.is_some(), "User should have remaining Dexy");
            assert_eq!(remaining_dexy.unwrap().amount, "900");

            assert_eq!(build.summary.direction, "dexy_to_erg");
            assert_eq!(build.summary.input_amount, 100);
            assert!(build.summary.output_amount > 0);
        }

        #[test]
        fn test_swap_summary_price_impact() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();

            let small_request = create_erg_to_dexy_request(
                1_000_000_000, // 1 ERG (0.1% of pool)
                1,
                100_000_000_000,
            );
            let small_result = build_swap_dexy_tx(&small_request, &ctx, &state).unwrap();

            let large_request = create_erg_to_dexy_request(
                100_000_000_000, // 100 ERG (10% of pool)
                1,
                200_000_000_000,
            );
            let large_result = build_swap_dexy_tx(&large_request, &ctx, &state).unwrap();

            assert!(
                large_result.summary.price_impact_pct > small_result.summary.price_impact_pct,
                "Large swap should have higher price impact ({} vs {})",
                large_result.summary.price_impact_pct,
                small_result.summary.price_impact_pct
            );
        }

        #[test]
        fn test_swap_lp_output_preserves_token_order() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(1_000_000_000, 1, 100_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
            let lp_output = &result.unsigned_tx.outputs[0];

            assert_eq!(lp_output.assets.len(), 3);
            assert_eq!(lp_output.assets[0].token_id, LP_NFT_ID);
            assert_eq!(lp_output.assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(lp_output.assets[2].token_id, DEXY_TOKEN_ID);
            assert_eq!(lp_output.assets[0].amount, "1");
            assert_eq!(lp_output.assets[1].amount, "9000000000000000");
        }

        #[test]
        fn test_swap_direction_enum() {
            assert_eq!(SwapDirection::ErgToDexy, SwapDirection::ErgToDexy);
            assert_ne!(SwapDirection::ErgToDexy, SwapDirection::DexyToErg);
        }

        #[test]
        fn test_swap_tx_summary_serialization() {
            let summary = SwapTxSummary {
                direction: "erg_to_dexy".to_string(),
                input_amount: 1_000_000_000,
                output_amount: 997,
                min_output: 990,
                price_impact_pct: 0.1,
                fee_pct: 0.3,
                miner_fee_nano: 1_100_000,
            };

            let json = serde_json::to_string(&summary).unwrap();
            assert!(json.contains("erg_to_dexy"));
            assert!(json.contains("1000000000"));

            let parsed: SwapTxSummary = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.direction, "erg_to_dexy");
            assert_eq!(parsed.input_amount, 1_000_000_000);
            assert_eq!(parsed.output_amount, 997);
        }

        #[test]
        fn test_dexy_to_erg_insufficient_erg_for_fees() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_dexy_to_erg_request(
                100,     // sell 100 Dexy
                1,       // min output
                100_000, // only 0.0001 ERG - not enough for fee + min box
                1000,    // enough Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient ERG"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_erg_to_dexy_change_output_when_needed() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(
                1_000_000_000, // swap 1 ERG
                1,
                100_000_000_000, // 100 ERG - lots of change
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
            let tx = &result.unsigned_tx;

            assert_eq!(tx.outputs.len(), 5, "Should have change output");
            let change = &tx.outputs[4];
            assert_eq!(change.ergo_tree, "user_ergo_tree");
            let change_erg: i64 = change.value.parse().unwrap();
            assert!(change_erg >= constants::MIN_BOX_VALUE_NANO);
        }
    }

    mod lp_deposit_redeem_tests {
        use super::*;
        use crate::fetch::DexyLpTxContext;

        const DEXY_TOKEN_ID: &str =
            "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
        const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
        const LP_TOKEN_ID: &str =
            "cf74432b2d3ab8a1a934b6326a1004e1a19aec7b357c57209018c4aa35226246";
        const LP_MINT_NFT_ID: &str =
            "19b8281b141d19c5b3843a4a77e616d6df05f601e5908159b1eaf3d9da20e664";
        const LP_REDEEM_NFT_ID: &str =
            "08c47eef5e782f146cae5e8cfb5e9d26b18442f82f3c5808b1563b6e3b23f729";
        const ORACLE_NFT_ID: &str =
            "3c45f29a5165b030fdb5eaf5d81f8108f9d8f507b31487dd51f4ae08fe07cf4a";

        const INITIAL_LP: i64 = 100_000_000_000; // Gold initial LP

        fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
            use ergo_lib::ergotree_ir::chain::ergo_box::{
                box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
            };
            use ergo_lib::ergotree_ir::chain::tx_id::TxId;
            use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
            use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

            let ergo_tree_bytes = base16::decode(
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            )
            .unwrap();
            let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).unwrap();
            let tx_id = TxId::zero();

            ErgoBox::new(
                BoxValue::new(1_000_000).unwrap(),
                ergo_tree,
                None,
                NonMandatoryRegisters::empty(),
                100000,
                tx_id,
                0,
            )
            .unwrap()
        }

        fn create_deposit_context(
            lp_erg: i64,
            lp_dexy: i64,
            lp_token_reserves: i64,
        ) -> DexyLpTxContext {
            let lp_input = Eip12InputBox {
                box_id: "lp_box_id".to_string(),
                transaction_id: "lp_tx_id".to_string(),
                index: 0,
                value: lp_erg.to_string(),
                ergo_tree: "lp_ergo_tree_hex".to_string(),
                assets: vec![
                    Eip12Asset::new(LP_NFT_ID, 1),
                    Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
                    Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
                ],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let action_input = Eip12InputBox {
                box_id: "mint_box_id".to_string(),
                transaction_id: "mint_tx_id".to_string(),
                index: 0,
                value: "1000000".to_string(),
                ergo_tree: "mint_ergo_tree_hex".to_string(),
                assets: vec![Eip12Asset::new(LP_MINT_NFT_ID, 1)],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let dummy_box = create_dummy_ergo_box();

            DexyLpTxContext {
                lp_input,
                lp_erg_reserves: lp_erg,
                lp_dexy_reserves: lp_dexy,
                lp_token_reserves,
                lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
                lp_box: dummy_box.clone(),
                lp_tokens: vec![
                    (LP_NFT_ID.to_string(), 1),
                    (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
                    (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
                ],
                action_input,
                action_erg_value: 1_000_000,
                action_ergo_tree: "mint_ergo_tree_hex".to_string(),
                action_box: dummy_box,
                action_tokens: vec![(LP_MINT_NFT_ID.to_string(), 1)],
                oracle_data_input: None,
                oracle_rate_nano: None,
            }
        }

        fn create_redeem_context(
            lp_erg: i64,
            lp_dexy: i64,
            lp_token_reserves: i64,
            oracle_rate_nano: i64,
        ) -> DexyLpTxContext {
            use ergo_tx::Eip12DataInputBox;

            let lp_input = Eip12InputBox {
                box_id: "lp_box_id".to_string(),
                transaction_id: "lp_tx_id".to_string(),
                index: 0,
                value: lp_erg.to_string(),
                ergo_tree: "lp_ergo_tree_hex".to_string(),
                assets: vec![
                    Eip12Asset::new(LP_NFT_ID, 1),
                    Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
                    Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
                ],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let action_input = Eip12InputBox {
                box_id: "redeem_box_id".to_string(),
                transaction_id: "redeem_tx_id".to_string(),
                index: 0,
                value: "1000000".to_string(),
                ergo_tree: "redeem_ergo_tree_hex".to_string(),
                assets: vec![Eip12Asset::new(LP_REDEEM_NFT_ID, 1)],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let oracle_data_input = Eip12DataInputBox {
                box_id: "oracle_box_id".to_string(),
                transaction_id: "oracle_tx_id".to_string(),
                index: 0,
                value: "1000000".to_string(),
                ergo_tree: "oracle_ergo_tree_hex".to_string(),
                assets: vec![Eip12Asset::new(ORACLE_NFT_ID, 1)],
                creation_height: 100000,
                additional_registers: HashMap::new(),
            };

            let dummy_box = create_dummy_ergo_box();

            DexyLpTxContext {
                lp_input,
                lp_erg_reserves: lp_erg,
                lp_dexy_reserves: lp_dexy,
                lp_token_reserves,
                lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
                lp_box: dummy_box.clone(),
                lp_tokens: vec![
                    (LP_NFT_ID.to_string(), 1),
                    (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
                    (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
                ],
                action_input,
                action_erg_value: 1_000_000,
                action_ergo_tree: "redeem_ergo_tree_hex".to_string(),
                action_box: dummy_box,
                action_tokens: vec![(LP_REDEEM_NFT_ID.to_string(), 1)],
                oracle_data_input: Some(oracle_data_input),
                oracle_rate_nano: Some(oracle_rate_nano),
            }
        }


        #[test]
        fn test_lp_deposit_rejects_zero_erg() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
            let request = LpDepositRequest {
                variant: DexyVariant::Gold,
                deposit_erg: 0,
                deposit_dexy: 100,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    100_000_000_000,
                    vec![(DEXY_TOKEN_ID, 1000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result =
                build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("ERG"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_lp_deposit_rejects_zero_dexy() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
            let request = LpDepositRequest {
                variant: DexyVariant::Gold,
                deposit_erg: 10_000_000_000,
                deposit_dexy: 0,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    100_000_000_000,
                    vec![(DEXY_TOKEN_ID, 1000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result =
                build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Dexy"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_lp_deposit_builds_correctly() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
            let request = LpDepositRequest {
                variant: DexyVariant::Gold,
                deposit_erg: 10_000_000_000,
                deposit_dexy: 5_000,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    100_000_000_000, // 100 ERG
                    vec![(DEXY_TOKEN_ID, 10_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result =
                build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            assert_eq!(tx.inputs.len(), 3);
            assert_eq!(tx.inputs[0].box_id, "lp_box_id");
            assert_eq!(tx.inputs[1].box_id, "mint_box_id");
            assert_eq!(tx.data_inputs.len(), 0);
            assert!(
                tx.outputs.len() >= 3,
                "Expected at least 3 outputs, got {}",
                tx.outputs.len()
            );

            assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
            let lp_erg_out: i64 = tx.outputs[0].value.parse().unwrap();
            assert!(lp_erg_out > 1_000_000_000_000, "LP ERG should increase after deposit");
            assert_eq!(tx.outputs[0].assets.len(), 3);
            assert_eq!(tx.outputs[0].assets[0].token_id, LP_NFT_ID);
            assert_eq!(tx.outputs[0].assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(tx.outputs[0].assets[2].token_id, DEXY_TOKEN_ID);
            let new_lp_reserves: i64 = tx.outputs[0].assets[1].amount.parse().unwrap();
            assert!(new_lp_reserves < 99_900_000_000, "LP token reserves should decrease");
            assert_eq!(tx.outputs[1].ergo_tree, "mint_ergo_tree_hex");
            assert_eq!(tx.outputs[1].value, "1000000");
            assert_eq!(tx.outputs[1].assets.len(), 1);
            assert_eq!(tx.outputs[1].assets[0].token_id, LP_MINT_NFT_ID);

            let user_output = &tx.outputs[2];
            assert_eq!(user_output.ergo_tree, "user_ergo_tree");
            assert!(
                user_output.assets.iter().any(|a| a.token_id == LP_TOKEN_ID),
                "User should receive LP tokens"
            );
            let lp_out: i64 = user_output
                .assets
                .iter()
                .find(|a| a.token_id == LP_TOKEN_ID)
                .unwrap()
                .amount
                .parse()
                .unwrap();
            assert!(lp_out > 0, "User should receive positive LP tokens");

            assert!(build.summary.action.starts_with("lp_deposit"));
            assert!(build.summary.erg_amount > 0);
            assert!(build.summary.dexy_amount > 0);
            assert!(build.summary.lp_tokens > 0);
        }

        #[test]
        fn test_lp_deposit_with_recipient() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

            let request = LpDepositRequest {
                variant: DexyVariant::Gold,
                deposit_erg: 10_000_000_000,
                deposit_dexy: 5_000,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    100_000_000_000,
                    vec![(DEXY_TOKEN_ID, 10_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: Some("recipient_ergo_tree".to_string()),
            };

            let result =
                build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let tx = &result.unwrap().unsigned_tx;
            assert_eq!(tx.outputs[2].ergo_tree, "recipient_ergo_tree");
        }


        #[test]
        fn test_lp_redeem_rejects_zero_lp() {
            // Oracle rate: 1,000,000,000,000 raw nanoERG/kg (for Gold, divisor = 1M -> 1,000,000 nanoERG/mg)
            // LP rate: 1,000,000,000,000 / 500,000 = 2,000,000 nanoERG/token
            // Oracle adjusted: 1,000,000 nanoERG/token
            // can_redeem: lp_rate(2M) > oracle_adjusted(1M) * 98/100 -> true
            let ctx = create_redeem_context(
                1_000_000_000_000,
                500_000,
                99_900_000_000,
                1_000_000_000_000, // raw oracle rate (nanoERG per kg)
            );

            let request = LpRedeemRequest {
                variant: DexyVariant::Gold,
                lp_to_burn: 0,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    10_000_000_000,
                    vec![(LP_TOKEN_ID, 1_000_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("positive"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_lp_redeem_blocked_by_oracle_gate() {
            // Set oracle rate very high so LP rate < 98% of oracle
            // LP rate: 1,000,000,000,000 / 500,000 = 2,000,000 nanoERG/token
            // Oracle raw: 3,000,000,000,000 -> adjusted: 3,000,000 nanoERG/token
            // can_redeem: lp_rate(2M) > oracle_adjusted(3M) * 98/100 = 2.94M -> false
            let ctx = create_redeem_context(
                1_000_000_000_000,
                500_000,
                99_900_000_000,
                3_000_000_000_000, // High oracle rate -> LP depeg -> blocked
            );

            let request = LpRedeemRequest {
                variant: DexyVariant::Gold,
                lp_to_burn: 1_000_000,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    10_000_000_000,
                    vec![(LP_TOKEN_ID, 1_000_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("depeg protection"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_lp_redeem_builds_correctly() {
            // Pool: 1000 ERG, 500K Dexy, 99.9B LP tokens reserved (100M circulating)
            // Oracle rate (raw): 1T nanoERG/kg -> adjusted: 1M nanoERG/mg
            // LP rate: 1T / 500K = 2M nanoERG/token
            // can_redeem: 2M > 1M * 98/100 = 980K -> true
            let ctx = create_redeem_context(
                1_000_000_000_000,
                500_000,
                99_900_000_000,
                1_000_000_000_000,
            );

            let request = LpRedeemRequest {
                variant: DexyVariant::Gold,
                lp_to_burn: 1_000_000,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    10_000_000_000, // 10 ERG for fees
                    vec![(LP_TOKEN_ID, 2_000_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            assert_eq!(tx.inputs.len(), 3);
            assert_eq!(tx.inputs[0].box_id, "lp_box_id");
            assert_eq!(tx.inputs[1].box_id, "redeem_box_id");
            assert_eq!(tx.data_inputs.len(), 1);
            assert_eq!(tx.data_inputs[0].box_id, "oracle_box_id");
            assert!(
                tx.outputs.len() >= 4,
                "Expected at least 4 outputs, got {}",
                tx.outputs.len()
            );

            assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
            let lp_erg_out: i64 = tx.outputs[0].value.parse().unwrap();
            assert!(lp_erg_out < 1_000_000_000_000, "LP ERG should decrease after redeem");
            let new_lp_reserves: i64 = tx.outputs[0].assets[1].amount.parse().unwrap();
            assert!(new_lp_reserves > 99_900_000_000, "LP token reserves should increase");
            assert_eq!(tx.outputs[1].ergo_tree, "redeem_ergo_tree_hex");
            assert_eq!(tx.outputs[1].value, "1000000");
            assert_eq!(tx.outputs[1].assets.len(), 1);
            assert_eq!(tx.outputs[1].assets[0].token_id, LP_REDEEM_NFT_ID);

            let user_output = &tx.outputs[2];
            assert_eq!(user_output.ergo_tree, "user_ergo_tree");
            let user_erg: i64 = user_output.value.parse().unwrap();
            assert!(user_erg > 0, "User should receive ERG");
            let dexy_asset = user_output
                .assets
                .iter()
                .find(|a| a.token_id == DEXY_TOKEN_ID);
            assert!(dexy_asset.is_some(), "User should receive Dexy tokens");
            let dexy_out: i64 = dexy_asset.unwrap().amount.parse().unwrap();
            assert!(dexy_out > 0, "User should receive positive Dexy tokens");

            assert!(build.summary.action.starts_with("lp_redeem"));
            assert!(build.summary.erg_amount > 0);
            assert!(build.summary.dexy_amount > 0);
            assert_eq!(build.summary.lp_tokens, 1_000_000);
        }

        #[test]
        fn test_lp_redeem_no_oracle_fails() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

            let request = LpRedeemRequest {
                variant: DexyVariant::Gold,
                lp_to_burn: 1_000_000,
                user_address: "user_addr".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    10_000_000_000,
                    vec![(LP_TOKEN_ID, 2_000_000)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            };

            let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Oracle"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_lp_deposit_summary_serialization() {
            let summary = LpTxSummary {
                action: "lp_deposit_gold".to_string(),
                erg_amount: 10_000_000_000,
                dexy_amount: 5_000,
                lp_tokens: 1_000_000,
                miner_fee_nano: 1_100_000,
            };

            let json = serde_json::to_string(&summary).unwrap();
            assert!(json.contains("lp_deposit_gold"));

            let parsed: LpTxSummary = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.action, "lp_deposit_gold");
            assert_eq!(parsed.lp_tokens, 1_000_000);
        }

        #[test]
        fn test_lp_pool_output_preserves_token_order() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

            let output = build_lp_pool_output(
                &ctx,
                1_010_000_000_000, // new ERG
                99_899_000_000,    // new LP token reserves
                505_000,           // new Dexy
                LP_TOKEN_ID,
                DEXY_TOKEN_ID,
                100001,
            );

            assert_eq!(output.assets.len(), 3);
            assert_eq!(output.assets[0].token_id, LP_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");
            assert_eq!(output.assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(output.assets[1].amount, "99899000000");
            assert_eq!(output.assets[2].token_id, DEXY_TOKEN_ID);
            assert_eq!(output.assets[2].amount, "505000");
            assert_eq!(output.value, "1010000000000");
        }

        #[test]
        fn test_action_nft_output_self_preservation() {
            let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

            let output = build_action_nft_output(&ctx, 100001);

            assert_eq!(output.value, "1000000");
            assert_eq!(output.ergo_tree, "mint_ergo_tree_hex");
            assert_eq!(output.assets.len(), 1);
            assert_eq!(output.assets[0].token_id, LP_MINT_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");
        }
    }
}
