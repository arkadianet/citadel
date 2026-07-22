//! SigmaFi use-case orchestration: bond market fetch, loan token list, order/bond tx building.

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

async fn oracle_erg_usd(state: &AppState) -> ServiceResult<f64> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let nft_ids = sigmausd::NftIds::for_network(config.network)
        .ok_or_else(|| format!("Oracle not available on {:?}", config.network))?;
    let price = sigmausd::fetch_oracle_price(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;
    Ok(price.erg_usd)
}

pub async fn fetch_market(
    state: &AppState,
    user_address: Option<&str>,
) -> ServiceResult<sigmafi::BondMarket> {
    let client = state.require_node_client().await?;
    let height = client.current_height().await.into_service()?;

    // Oracle price is optional context for collateral ratios; ignore failures.
    let oracle_erg_usd = oracle_erg_usd(state).await.ok();

    sigmafi::fetch_bond_market(&client, user_address, height as u32, oracle_erg_usd)
        .await
        .into_service()
}

pub fn get_tokens() -> Vec<serde_json::Value> {
    sigmafi::SUPPORTED_TOKENS
        .iter()
        .map(|t| {
            serde_json::json!({
                "token_id": t.token_id,
                "name": t.name,
                "decimals": t.decimals,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build_open_order(
    borrower_ergo_tree: String,
    loan_token_id: String,
    principal: u64,
    repayment: u64,
    maturity_blocks: i32,
    collateral_erg: u64,
    collateral_tokens: Vec<(String, u64)>,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let req = sigmafi::tx_builder::OpenOrderRequest {
        borrower_ergo_tree,
        loan_token_id,
        principal,
        repayment,
        maturity_blocks,
        collateral_erg,
        collateral_tokens,
        user_inputs,
        current_height,
    };

    sigmafi::tx_builder::build_open_order(&req).into_service()
}

pub async fn build_cancel_order(
    state: &AppState,
    box_id: &str,
    borrower_ergo_tree: String,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;
    let order_box = client
        .get_eip12_box_by_id(box_id)
        .await
        .map_err(|e| format!("Failed to fetch order box: {}", e))?;

    let req = sigmafi::tx_builder::CancelOrderRequest {
        order_box,
        borrower_ergo_tree,
        user_inputs,
        current_height,
    };

    sigmafi::tx_builder::build_cancel_order(&req).into_service()
}

#[allow(clippy::too_many_arguments)]
pub async fn build_close_order(
    state: &AppState,
    box_id: &str,
    lender_ergo_tree: String,
    ui_fee_ergo_tree: String,
    loan_token_id: String,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;
    let order_box = client
        .get_eip12_box_by_id(box_id)
        .await
        .map_err(|e| format!("Failed to fetch order box: {}", e))?;

    let req = sigmafi::tx_builder::CloseOrderRequest {
        order_box,
        lender_ergo_tree,
        ui_fee_ergo_tree,
        loan_token_id,
        user_inputs,
        current_height,
    };

    sigmafi::tx_builder::build_close_order(&req).into_service()
}

pub async fn build_repay(
    state: &AppState,
    box_id: &str,
    loan_token_id: String,
    borrower_ergo_tree: String,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;
    let bond_box = client
        .get_eip12_box_by_id(box_id)
        .await
        .map_err(|e| format!("Failed to fetch bond box: {}", e))?;

    let req = sigmafi::tx_builder::RepayRequest {
        bond_box,
        loan_token_id,
        borrower_ergo_tree,
        user_inputs,
        current_height,
    };

    sigmafi::tx_builder::build_repay(&req).into_service()
}

pub async fn build_liquidate(
    state: &AppState,
    box_id: &str,
    lender_ergo_tree: String,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;
    let bond_box = client
        .get_eip12_box_by_id(box_id)
        .await
        .map_err(|e| format!("Failed to fetch bond box: {}", e))?;

    let req = sigmafi::tx_builder::LiquidateRequest {
        bond_box,
        lender_ergo_tree,
        user_inputs,
        current_height,
    };

    sigmafi::tx_builder::build_liquidate(&req).into_service()
}
