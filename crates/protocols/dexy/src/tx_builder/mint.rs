use std::collections::HashMap;

use citadel_core::{constants, TxError};
use ergo_tx::{
    append_change_output, append_dev_fee_output, resolved_dev_fee_config, select_inputs_for_spend,
    Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::constants::{DexyVariant, BANK_FEE_NUM, BUYBACK_FEE_NUM, FEE_DENOM};
use crate::fetch::DexyTxContext;
use crate::state::DexyState;

use super::validate::{validate_free_mint_preflight, T_BUFFER};

/// FreeMint period length in blocks (1/2 day on mainnet)
const T_FREE: i32 = 360;
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
    pub citadel_fee_nano: i64,
    pub bank_fee_nano: i64,
    pub buyback_fee_nano: i64,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: TxSummary,
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
pub(crate) fn calculate_mint_amounts(amount: i64, oracle_rate_nano: i64, variant: DexyVariant) -> (i64, i64) {
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
    let fee_cfg = resolved_dev_fee_config();
    let citadel_fee = fee_cfg.budget();
    let total_cost = bank_erg_added
        + buyback_fee
        + constants::TX_FEE_NANO
        + citadel_fee
        + constants::MIN_BOX_VALUE_NANO;

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
    ];

    let erg_used = total_cost as u64;
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

    append_dev_fee_output(&mut outputs, &fee_cfg, request.current_height).map_err(|e| {
        TxError::BuildFailed {
            message: e.to_string(),
        }
    })?;
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

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
        citadel_fee_nano: citadel_fee,
        bank_fee_nano: bank_fee,
        buyback_fee_nano: buyback_fee,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

pub(crate) fn build_free_mint_output(ctx: &DexyTxContext, new_r4: i32, new_r5: i64, height: i32) -> Eip12Output {
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
pub(crate) fn build_bank_output(ctx: &DexyTxContext, new_erg: i64, new_dexy_tokens: i64, height: i32) -> Eip12Output {
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
pub(crate) fn build_buyback_output(ctx: &DexyTxContext, new_erg: i64, height: i32) -> Result<Eip12Output, TxError> {
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
