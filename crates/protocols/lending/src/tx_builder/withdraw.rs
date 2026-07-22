use crate::constants::PoolConfig;
use ergo_tx::{
    append_change_output,
    sigma::{encode_sigma_coll_byte, encode_sigma_long},
    Eip12Asset, Eip12InputBox, Eip12Output,
};

use super::common::{
    finalize_proxy_tx, miner_fee_output, resolve_user_ergo_tree, select_token_inputs,
    to_ergo_tx_selected, user_utxo_to_eip12,
};
use super::{
    BuildError, BuildResponse, TxSummary, WithdrawRequest, MIN_BOX_VALUE_NANO,
    PROXY_EXECUTION_FEE_NANO, REFUND_HEIGHT_OFFSET, TX_FEE_NANO,
};

/// Proxy registers: R4=user ErgoTree, R5=min output, R6=refund height, R7=currency ID (token pools)
pub fn build_withdraw_tx(
    req: WithdrawRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    if req.lp_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "LP amount must be greater than 0".to_string(),
        ));
    }
    if req.lp_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "LP amount exceeds maximum supported value".to_string(),
        ));
    }

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;
    let proxy_ergo_tree =
        ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.withdraw_address)
            .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    let inputs = select_token_inputs(
        &req.user_utxos,
        config.lend_token_id,
        req.lp_amount as i64,
        total_required,
    )?;

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.min_output.unwrap_or(0) as i64),
        "R6" => encode_sigma_long(refund_height as i64),
    );

    if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            let currency_bytes = hex::decode(currency_id)
                .map_err(|e| BuildError::TxBuildError(format!("Invalid currency ID: {}", e)))?;
            proxy_registers.insert("R7".to_string(), encode_sigma_coll_byte(&currency_bytes));
        }
    }

    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: vec![Eip12Asset {
            token_id: config.lend_token_id.to_string(),
            amount: req.lp_amount.to_string(),
        }],
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = vec![(config.lend_token_id, req.lp_amount)];
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

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "withdraw".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{} LP", req.lp_amount),
            amount_out_estimate: None,
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: 0,
            total_to_send_display: String::new(),
        },
    })
}
