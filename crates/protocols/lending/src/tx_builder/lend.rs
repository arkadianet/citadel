use crate::constants::PoolConfig;
use ergo_tx::{
    append_change_output,
    sigma::{encode_sigma_coll_byte, encode_sigma_long},
    Eip12Asset, Eip12InputBox, Eip12Output,
};

use super::common::{
    finalize_proxy_tx, miner_fee_output, resolve_user_ergo_tree, select_erg_inputs,
    select_token_inputs, to_ergo_tx_selected, user_utxo_to_eip12,
};
use super::{
    BuildError, BuildResponse, LendRequest, TxSummary, BOT_PROCESSING_OVERHEAD, MIN_BOX_VALUE_NANO,
    REFUND_HEIGHT_OFFSET, TX_FEE_NANO,
};

/// The bot deducts a service fee from whatever tokens are in the proxy box, so we must
/// include amount + service_fee (+ optional slippage buffer) in the proxy box.
///
/// Proxy registers: R4=user ErgoTree, R5=min LP tokens, R6=refund height, R7=lend token ID (token pools)
pub fn build_lend_tx(
    req: LendRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    use crate::calculator;

    if req.amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Amount must be greater than 0".to_string(),
        ));
    }
    if req.amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Amount exceeds maximum supported value".to_string(),
        ));
    }

    let slippage_bps = req.slippage_bps.min(200);

    let service_fee = calculator::calculate_service_fee(req.amount, config.is_erg_pool);
    // Min fee: 1 token unit for token pools, MIN_BOX_VALUE_NANO for ERG pools
    let service_fee = if config.is_erg_pool {
        service_fee.max(MIN_BOX_VALUE_NANO as u64)
    } else {
        service_fee.max(1)
    };

    let slippage_buffer = req.amount * slippage_bps as u64 / 10000;

    let total_to_send = req
        .amount
        .checked_add(service_fee)
        .and_then(|v| v.checked_add(slippage_buffer))
        .ok_or_else(|| {
            BuildError::TxBuildError("Amount overflow in total_to_send calculation".to_string())
        })?;

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let proxy_ergo_tree =
        ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.lend_address)
            .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = if config.is_erg_pool {
        (total_to_send as i64) + BOT_PROCESSING_OVERHEAD
    } else {
        BOT_PROCESSING_OVERHEAD
    };

    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    let inputs = if config.is_erg_pool {
        select_erg_inputs(&req.user_utxos, total_required)?
    } else {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError("Token pool missing currency_id".to_string())
        })?;
        select_token_inputs(
            &req.user_utxos,
            currency_id,
            total_to_send as i64,
            total_required,
        )?
    };

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.min_lp_tokens.unwrap_or(0) as i64),
        "R6" => encode_sigma_long(refund_height as i64),
    );

    if !config.is_erg_pool {
        let lend_token_bytes = hex::decode(config.lend_token_id)
            .map_err(|e| BuildError::TxBuildError(format!("Invalid lend token ID: {}", e)))?;
        proxy_registers.insert("R7".to_string(), encode_sigma_coll_byte(&lend_token_bytes));
    }

    let mut proxy_assets = Vec::new();
    if !config.is_erg_pool {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError(
                "Pool marked as non-ERG but currency_id is missing".to_string(),
            )
        })?;
        proxy_assets.push(Eip12Asset {
            token_id: currency_id.to_string(),
            amount: total_to_send.to_string(),
        });
    }

    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            vec![(currency_id, total_to_send)]
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &user_ergo_tree,
        current_height,
        MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| BuildError::TxBuildError(e.to_string()))?;

    let (unsigned_tx_json, proxy_address) =
        finalize_proxy_tx(eip12_inputs, outputs, &proxy_ergo_tree)?;

    let divisor = 10f64.powi(config.decimals as i32);
    let amount_display = (req.amount as f64) / divisor;
    let fee_display = (service_fee as f64) / divisor;
    let total_display = (total_to_send as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "lend".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{:.6} {}", amount_display, config.symbol),
            amount_out_estimate: None,
            proxy_address,
            refund_height,
            service_fee_raw: service_fee,
            service_fee_display: format!("{:.6} {}", fee_display, config.symbol),
            total_to_send_raw: total_to_send,
            total_to_send_display: format!("{:.6} {}", total_display, config.symbol),
        },
    })
}
