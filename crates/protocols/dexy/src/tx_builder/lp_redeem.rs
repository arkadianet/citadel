use citadel_core::{constants, TxError};
use ergo_tx::{
    append_dev_fee_output, collect_change_tokens, resolved_dev_fee_config, select_inputs_for_spend,
    Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{calculate_lp_redeem, can_redeem_lp};
use crate::constants::DexyVariant;
use crate::fetch::DexyLpTxContext;

use super::lp_deposit::{build_action_nft_output, build_lp_pool_output, LpBuildResult, LpTxSummary};

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

    let fee_cfg = resolved_dev_fee_config();
    let citadel_fee = fee_cfg.budget();
    let min_erg = constants::TX_FEE_NANO + citadel_fee + constants::MIN_BOX_VALUE_NANO;
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

    let user_output_erg =
        selected.total_erg as i64 + calc.erg_out - constants::TX_FEE_NANO - citadel_fee;
    let mut outputs = vec![
        build_lp_pool_output(ctx, new_lp_erg, new_lp_token_reserves, new_lp_dexy, lp_token_id, dexy_token_id, request.current_height),
        build_action_nft_output(ctx, request.current_height),
        Eip12Output::change(user_output_erg, output_ergo_tree, user_assets, request.current_height),
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
        data_inputs,
        outputs,
    };

    let summary = LpTxSummary {
        action: format!("lp_redeem_{}", request.variant.as_str()),
        erg_amount: calc.erg_out,
        dexy_amount: calc.dexy_out,
        lp_tokens: request.lp_to_burn,
        miner_fee_nano: constants::TX_FEE_NANO,
        citadel_fee_nano: citadel_fee,
    };

    Ok(LpBuildResult {
        unsigned_tx,
        summary,
    })
}
