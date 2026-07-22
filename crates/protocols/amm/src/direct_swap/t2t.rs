//! T2T direct swap builder.

use crate::calculator;
use crate::constants::fees;
use crate::state::{AmmError, AmmPool, SwapInput};
use crate::tx_builder::MIN_CHANGE_VALUE;
use ergo_tx::{
    append_dev_fee_output, collect_change_tokens, resolved_dev_fee_config, select_token_boxes,
    Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use super::{DirectSwapBuildResult, DirectSwapSummary, MIN_BOX_VALUE};

/// T2T pools: 4 tokens [NFT, LP, X, Y], ERG unchanged (storage rent only).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_t2t_direct_swap(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    input: &SwapInput,
    min_output: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    recipient_ergo_tree: Option<&str>,
    miner_fee: u64,
) -> Result<DirectSwapBuildResult, AmmError> {
    let (input_token_id, input_amount) = match input {
        SwapInput::Erg { .. } => {
            return Err(AmmError::InvalidToken(
                "ERG input is not valid for T2T pools -- use a token".to_string(),
            ));
        }
        SwapInput::Token { token_id, amount } => (token_id.as_str(), *amount),
    };

    let pool_erg: u64 = pool_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool box ERG value".to_string()))?;

    if pool_box.assets.len() < 4 {
        return Err(AmmError::TxBuildError(format!(
            "T2T pool box has {} tokens, expected at least 4",
            pool_box.assets.len()
        )));
    }

    let pool_nft = &pool_box.assets[0];
    let pool_lp = &pool_box.assets[1];
    let pool_token_x_asset = &pool_box.assets[2];
    let pool_token_y_asset = &pool_box.assets[3];

    let pool_token_x_amount: u64 = pool_token_x_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token X amount".to_string()))?;
    let pool_token_y_amount: u64 = pool_token_y_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    let token_x_meta = pool
        .token_x
        .as_ref()
        .ok_or_else(|| AmmError::TxBuildError("T2T pool missing token_x metadata".to_string()))?;

    let fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;
    let fee_denom = fees::DEFAULT_FEE_DENOM;

    let is_x_to_y = if input_token_id == token_x_meta.token_id {
        true
    } else if input_token_id == pool.token_y.token_id {
        false
    } else {
        return Err(AmmError::InvalidToken(format!(
            "Token {} does not match pool token_x ({}) or token_y ({})",
            input_token_id,
            &token_x_meta.token_id[..8],
            &pool.token_y.token_id[..8],
        )));
    };

    let (reserves_in, reserves_out) = if is_x_to_y {
        (pool_token_x_amount, pool_token_y_amount)
    } else {
        (pool_token_y_amount, pool_token_x_amount)
    };

    let output_amount =
        calculator::calculate_output(reserves_in, reserves_out, input_amount, fee_num, fee_denom);
    if output_amount == 0 {
        return Err(AmmError::InsufficientLiquidity);
    }

    if output_amount < min_output {
        return Err(AmmError::SlippageExceeded {
            got: output_amount,
            min: min_output,
        });
    }

    let (new_token_x_amount, new_token_y_amount) = if is_x_to_y {
        let new_x = pool_token_x_amount
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token X overflow".to_string()))?;
        let new_y = pool_token_y_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;
        (new_x, new_y)
    } else {
        let new_x = pool_token_x_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token X underflow".to_string()))?;
        let new_y = pool_token_y_amount
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y overflow".to_string()))?;
        (new_x, new_y)
    };

    let new_pool_output = Eip12Output {
        value: pool_erg.to_string(),
        ergo_tree: pool_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: pool_nft.token_id.clone(),
                amount: pool_nft.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_lp.token_id.clone(),
                amount: pool_lp.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_token_x_asset.token_id.clone(),
                amount: new_token_x_amount.to_string(),
            },
            Eip12Asset {
                token_id: pool_token_y_asset.token_id.clone(),
                amount: new_token_y_amount.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    let output_token_id = if is_x_to_y {
        &pool.token_y.token_id
    } else {
        &token_x_meta.token_id
    };

    let output_tree = recipient_ergo_tree.unwrap_or(user_ergo_tree);
    let user_swap_output = Eip12Output::change(
        MIN_BOX_VALUE as i64,
        output_tree,
        vec![Eip12Asset {
            token_id: output_token_id.clone(),
            amount: output_amount.to_string(),
        }],
        current_height,
    );

    let fee_cfg = resolved_dev_fee_config();
    let citadel_fee = fee_cfg.budget() as u64;
    let fee_output = Eip12Output::fee(miner_fee as i64, current_height);

    let user_erg_needed = MIN_BOX_VALUE
        .checked_add(miner_fee)
        .and_then(|v| v.checked_add(citadel_fee))
        .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

    let selected = select_token_boxes(user_utxos, input_token_id, input_amount, user_erg_needed)
        .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((input_token_id, input_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    let mut outputs = vec![new_pool_output];

    if recipient_ergo_tree.is_none() {
        let user_erg = MIN_BOX_VALUE + change_erg;

        let mut user_tokens = vec![Eip12Asset {
            token_id: output_token_id.clone(),
            amount: output_amount.to_string(),
        }];
        user_tokens.extend(change_tokens);

        outputs.push(Eip12Output::change(
            user_erg as i64,
            user_ergo_tree,
            user_tokens,
            current_height,
        ));
    } else {
        if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
            return Err(AmmError::TxBuildError(format!(
                "Change tokens exist but not enough ERG for change box (need {}, have {})",
                MIN_CHANGE_VALUE, change_erg
            )));
        }

        // If change ERG is too small for a separate box and there are no change
        // tokens, fold it into the swap output to avoid losing ERG.
        let user_swap_output =
            if change_erg > 0 && change_erg < MIN_CHANGE_VALUE && change_tokens.is_empty() {
                let base_value: u64 = user_swap_output
                    .value
                    .parse()
                    .map_err(|_| AmmError::TxBuildError("Invalid swap output value".to_string()))?;
                Eip12Output {
                    value: (base_value + change_erg).to_string(),
                    ..user_swap_output
                }
            } else {
                user_swap_output
            };

        outputs.push(user_swap_output);

        if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
            outputs.push(Eip12Output::change(
                change_erg as i64,
                user_ergo_tree,
                change_tokens,
                current_height,
            ));
        }
    }

    append_dev_fee_output(&mut outputs, &fee_cfg, current_height)
        .map_err(|e| AmmError::TxBuildError(e.to_string()))?;
    outputs.push(fee_output);

    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let (input_token_name, output_token_name) = if is_x_to_y {
        (
            token_x_meta
                .name
                .clone()
                .unwrap_or_else(|| token_x_meta.token_id[..8].to_string()),
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
        )
    } else {
        (
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
            token_x_meta
                .name
                .clone()
                .unwrap_or_else(|| token_x_meta.token_id[..8].to_string()),
        )
    };

    let summary = DirectSwapSummary {
        input_amount,
        input_token: input_token_name,
        output_amount,
        min_output,
        output_token: output_token_name,
        miner_fee,
        citadel_fee_nano: citadel_fee,
        total_erg_cost: user_erg_needed,
    };

    Ok(DirectSwapBuildResult {
        unsigned_tx,
        summary,
    })
}
