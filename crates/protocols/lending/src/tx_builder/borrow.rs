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
    BorrowRequest, BuildError, BuildResponse, TxSummary, MIN_BOX_VALUE_NANO,
    PROXY_EXECUTION_FEE_NANO, REFUND_HEIGHT_OFFSET, TX_FEE_NANO,
};

/// ERG pool: token collateral in proxy box, borrows ERG.
/// Token pool: ERG collateral in proxy value, borrows tokens.
///
/// Proxy registers: R4=user ErgoTree, R5=requestAmount, R6=refundHeight(Int),
/// R7=(threshold,penalty), R8=dexNft, R9=userPk(GroupElement)
pub fn build_borrow_tx(
    req: BorrowRequest,
    config: &PoolConfig,
    collateral_config: &crate::state::CollateralOption,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    if req.borrow_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Borrow amount must be greater than 0".to_string(),
        ));
    }
    if req.collateral_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Collateral amount must be greater than 0".to_string(),
        ));
    }

    if config.proxy_contracts.borrow_address.is_empty() {
        return Err(BuildError::ProxyContractMissing(config.id.to_string()));
    }

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let user_pk = ergo_tx::sigma::extract_pk_from_p2pk_ergo_tree(&user_ergo_tree).map_err(|e| {
        BuildError::InvalidAddress(format!(
            "Address must be a P2PK address (not a script): {}",
            e
        ))
    })?;

    let proxy_ergo_tree =
        ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.borrow_address)
            .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let (proxy_value, inputs) = if config.is_erg_pool {
        let proxy_val = MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
        let total_required = proxy_val + TX_FEE_NANO + MIN_BOX_VALUE_NANO;
        let selected = select_token_inputs(
            &req.user_utxos,
            &req.collateral_token,
            req.collateral_amount as i64,
            total_required,
        )?;
        (proxy_val, selected)
    } else {
        let proxy_val =
            (req.collateral_amount as i64) + MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
        let total_required = proxy_val + TX_FEE_NANO + MIN_BOX_VALUE_NANO;
        let selected = select_erg_inputs(&req.user_utxos, total_required)?;
        (proxy_val, selected)
    };

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.borrow_amount as i64),
        "R6" => ergo_tx::sigma::encode_sigma_int(refund_height),
        "R7" => ergo_tx::sigma::encode_sigma_long_pair(
            collateral_config.liquidation_threshold as i64,
            collateral_config.liquidation_penalty as i64,
        ),
    );

    let dex_nft_hex = collateral_config.dex_nft.as_deref().ok_or_else(|| {
        BuildError::TxBuildError("Collateral option missing DEX NFT for pricing".to_string())
    })?;
    let dex_nft_bytes = hex::decode(dex_nft_hex)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid DEX NFT hex: {}", e)))?;
    proxy_registers.insert("R8".to_string(), encode_sigma_coll_byte(&dex_nft_bytes));

    proxy_registers.insert(
        "R9".to_string(),
        ergo_tx::sigma::encode_sigma_group_element(&user_pk),
    );

    let mut proxy_assets = Vec::new();
    if config.is_erg_pool {
        proxy_assets.push(Eip12Asset {
            token_id: req.collateral_token.clone(),
            amount: req.collateral_amount.to_string(),
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
    let spent_tokens: Vec<(&str, u64)> = if config.is_erg_pool {
        vec![(&req.collateral_token, req.collateral_amount)]
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
    let borrow_display = (req.borrow_amount as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "borrow".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{} collateral", req.collateral_amount),
            amount_out_estimate: Some(format!("{:.6} {}", borrow_display, config.symbol)),
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: req.collateral_amount,
            total_to_send_display: format!("{} collateral", req.collateral_amount),
        },
    })
}
