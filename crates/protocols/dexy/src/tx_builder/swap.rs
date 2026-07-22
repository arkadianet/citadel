use serde::{Deserialize, Serialize};

use citadel_core::{constants, TxError};
use ergo_tx::{
    append_change_output, append_dev_fee_output, collect_change_tokens, resolved_dev_fee_config,
    select_inputs_for_spend, Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{
    calculate_lp_swap_output, calculate_lp_swap_price_impact, validate_lp_swap,
};
use crate::constants::{DexyVariant, LP_SWAP_FEE_DENOM, LP_SWAP_FEE_NUM};
use crate::fetch::DexySwapTxContext;
use crate::state::DexyState;

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
    pub citadel_fee_nano: i64,
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

    let fee_cfg = resolved_dev_fee_config();
    let citadel_fee = fee_cfg.budget();

    let selected = match request.direction {
        SwapDirection::ErgToDexy => {
            let needed = request.input_amount
                + constants::TX_FEE_NANO
                + citadel_fee
                + constants::MIN_BOX_VALUE_NANO;
            select_inputs_for_spend(&request.user_inputs, needed as u64, None)
        }
        SwapDirection::DexyToErg => {
            let min_erg = constants::TX_FEE_NANO + citadel_fee + constants::MIN_BOX_VALUE_NANO;
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

            let erg_used =
                (request.input_amount + constants::TX_FEE_NANO + citadel_fee + user_output_erg)
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
        }
        SwapDirection::DexyToErg => {
            let user_output_erg = selected.total_erg as i64 + output_amount
                - constants::TX_FEE_NANO
                - citadel_fee;
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
        }
    }

    append_dev_fee_output(&mut outputs, &fee_cfg, request.current_height).map_err(|e| {
        TxError::BuildFailed {
            message: e.to_string(),
        }
    })?;
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

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
        citadel_fee_nano: citadel_fee,
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
pub(crate) fn build_lp_swap_output(
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
pub(crate) fn build_swap_nft_output(ctx: &DexySwapTxContext, height: i32) -> Eip12Output {
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
