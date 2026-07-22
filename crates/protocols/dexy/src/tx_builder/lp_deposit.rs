use serde::{Deserialize, Serialize};

use citadel_core::{constants, TxError};
use ergo_tx::{
    append_dev_fee_output, collect_change_tokens, resolved_dev_fee_config, select_inputs_for_spend,
    Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::calculate_lp_deposit;
use crate::constants::DexyVariant;
use crate::fetch::DexyLpTxContext;

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

/// Summary of an LP deposit or redeem transaction for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpTxSummary {
    pub action: String,
    pub erg_amount: i64,
    pub dexy_amount: i64,
    pub lp_tokens: i64,
    pub miner_fee_nano: i64,
    pub citadel_fee_nano: i64,
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
pub(crate) fn build_lp_pool_output(
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
pub(crate) fn build_action_nft_output(ctx: &DexyLpTxContext, height: i32) -> Eip12Output {
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

    // 3. Select user UTXOs: need consumed_erg + TX_FEE + citadel + MIN_BOX_VALUE, and consumed_dexy
    let fee_cfg = resolved_dev_fee_config();
    let citadel_fee = fee_cfg.budget();
    let min_erg =
        calc.consumed_erg + constants::TX_FEE_NANO + citadel_fee + constants::MIN_BOX_VALUE_NANO;
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

    let user_erg =
        selected.total_erg as i64 - calc.consumed_erg - constants::TX_FEE_NANO - citadel_fee;
    let mut outputs = vec![
        build_lp_pool_output(ctx, new_lp_erg, new_lp_token_reserves, new_lp_dexy, lp_token_id, dexy_token_id, request.current_height),
        build_action_nft_output(ctx, request.current_height),
        Eip12Output::change(user_erg.max(constants::MIN_BOX_VALUE_NANO), output_ergo_tree, user_tokens, request.current_height),
    ];
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
        data_inputs: vec![],
        outputs,
    };

    let summary = LpTxSummary {
        action: format!("lp_deposit_{}", request.variant.as_str()),
        erg_amount: calc.consumed_erg,
        dexy_amount: calc.consumed_dexy,
        lp_tokens: calc.lp_tokens_out,
        miner_fee_nano: constants::TX_FEE_NANO,
        citadel_fee_nano: citadel_fee,
    };

    Ok(LpBuildResult {
        unsigned_tx,
        summary,
    })
}
