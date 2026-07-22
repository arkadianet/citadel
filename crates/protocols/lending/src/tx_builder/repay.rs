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
    BuildError, BuildResponse, RepayRequest, TxSummary, BOT_PROCESSING_OVERHEAD,
    MIN_BOX_VALUE_NANO, REFUND_HEIGHT_OFFSET, TX_FEE_NANO,
};

/// Proxy registers: R4=neededAmount(0), R5=borrower ErgoTree, R6=refundHeight(Int), R7=collateralBoxId
pub fn build_repay_tx(
    req: RepayRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    if req.repay_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Repay amount must be greater than 0".to_string(),
        ));
    }
    if req.repay_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Repay amount exceeds maximum supported value".to_string(),
        ));
    }
    if req.collateral_box_id.len() != 64 {
        return Err(BuildError::InvalidAmount(
            "Invalid collateral box ID: must be 64 hex characters".to_string(),
        ));
    }

    let collateral_box_bytes = hex::decode(&req.collateral_box_id)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid collateral box ID hex: {}", e)))?;

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let is_full_repay = req.repay_amount >= req.total_owed || req.total_owed == 0;
    let proxy_address = if is_full_repay {
        config.proxy_contracts.repay_address
    } else {
        if config.proxy_contracts.partial_repay_address.is_empty() {
            return Err(BuildError::ProxyContractMissing(format!(
                "Partial repay proxy not configured for pool: {}. \
                 Please repay the full amount ({}) or wait for partial repay support.",
                config.id, req.total_owed
            )));
        }
        config.proxy_contracts.partial_repay_address
    };
    let proxy_ergo_tree = ergo_tx::address::address_to_ergo_tree(proxy_address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = if config.is_erg_pool {
        (req.repay_amount as i64) + BOT_PROCESSING_OVERHEAD
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
            req.repay_amount as i64,
            total_required,
        )?
    };

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!("R4" => encode_sigma_long(0));
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_coll_byte(&user_ergo_tree_bytes),
    );
    // R6 must be SInt not SLong — contract reads SELF.R6[Int].get
    proxy_registers.insert(
        "R6".to_string(),
        ergo_tx::sigma::encode_sigma_int(refund_height),
    );
    proxy_registers.insert(
        "R7".to_string(),
        encode_sigma_coll_byte(&collateral_box_bytes),
    );

    let mut proxy_assets = Vec::new();
    if !config.is_erg_pool {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError(
                "Pool marked as non-ERG but currency_id is missing".to_string(),
            )
        })?;
        proxy_assets.push(Eip12Asset {
            token_id: currency_id.to_string(),
            amount: req.repay_amount.to_string(),
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
            vec![(currency_id, req.repay_amount)]
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
    let amount_display = (req.repay_amount as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "repay".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{:.6} {}", amount_display, config.symbol),
            amount_out_estimate: Some("Collateral returned".to_string()),
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: 0,
            total_to_send_display: String::new(),
        },
    })
}
