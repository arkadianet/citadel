use citadel_api::AppState;
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;

use super::StrErr;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use tauri::State;

fn ergo_tree_to_address(ergo_tree_hex: &str) -> Option<String> {
    let bytes = hex::decode(ergo_tree_hex).ok()?;
    let tree = ErgoTree::sigma_parse_bytes(&bytes).ok()?;
    let addr = Address::recreate_from_ergo_tree(&tree).ok()?;
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Some(encoder.address_to_str(&addr))
}

fn enrich_addresses_from_ergo_tree(boxes: &mut [serde_json::Value]) {
    for b in boxes.iter_mut() {
        if let Some(obj) = b.as_object_mut() {
            if obj
                .get("address")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .is_some()
            {
                continue;
            }
            if let Some(tree_hex) = obj.get("ergoTree").and_then(|v| v.as_str()) {
                if let Some(addr) = ergo_tree_to_address(tree_hex) {
                    obj.insert("address".to_string(), serde_json::Value::String(addr));
                }
            }
        }
    }
}

#[tauri::command]
pub async fn explorer_node_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;
    client.get_full_node_info().await.str_err()
}

/// Mempool inputs lack value/assets/address — enrich them by looking up spent boxes.
#[tauri::command]
pub async fn explorer_get_transaction(
    state: State<'_, AppState>,
    tx_id: String,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;

    fn is_valid_tx(v: &serde_json::Value) -> bool {
        v.get("id").is_some() && v.get("inputs").is_some()
    }
    if let Ok(mut tx) = client.get_transaction_by_id(&tx_id).await {
        if is_valid_tx(&tx) {
            if let Some(inputs) = tx.get_mut("inputs").and_then(|v| v.as_array_mut()) {
                enrich_addresses_from_ergo_tree(inputs);
            }
            if let Some(outputs) = tx.get_mut("outputs").and_then(|v| v.as_array_mut()) {
                enrich_addresses_from_ergo_tree(outputs);
            }
            return Ok(tx);
        }
    }

    let mut utx = client
        .get_unconfirmed_transaction_by_id(&tx_id)
        .await
        .map_err(|e| format!("Transaction not found: {}", e))?;
    if !is_valid_tx(&utx) {
        return Err(format!("Transaction not found: {}", tx_id));
    }

    if let Some(inputs) = utx.get_mut("inputs").and_then(|v| v.as_array_mut()) {
        let box_ids: Vec<String> = inputs
            .iter()
            .filter_map(|inp| inp.get("boxId").and_then(|v| v.as_str()).map(String::from))
            .collect();

        let futs: Vec<_> = box_ids
            .iter()
            .map(|id| client.get_blockchain_box_by_id(id))
            .collect();
        let results = futures::future::join_all(futs).await;

        for (input, result) in inputs.iter_mut().zip(results) {
            if let Ok(box_data) = result {
                if let Some(obj) = input.as_object_mut() {
                    if let Some(v) = box_data.get("value") {
                        obj.insert("value".to_string(), v.clone());
                    }
                    if let Some(a) = box_data.get("assets") {
                        obj.insert("assets".to_string(), a.clone());
                    }
                    if let Some(t) = box_data.get("ergoTree") {
                        obj.insert("ergoTree".to_string(), t.clone());
                    }
                    if let Some(addr) = box_data.get("address") {
                        obj.insert("address".to_string(), addr.clone());
                    }
                }
            }
        }
    }

    if let Some(outputs) = utx.get_mut("outputs").and_then(|v| v.as_array_mut()) {
        enrich_addresses_from_ergo_tree(outputs);
    }

    Ok(utx)
}

#[tauri::command]
pub async fn explorer_get_block(
    state: State<'_, AppState>,
    block_id: String,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;

    if block_id.chars().all(|c| c.is_ascii_digit()) {
        let height: u64 = block_id.parse().map_err(|_| "Invalid block height")?;
        let ids = client
            .get_block_ids_at_height(height)
            .await
            .str_err()?;
        let header_id = ids.first().ok_or("No block at this height")?;
        client
            .get_block_by_id(header_id)
            .await
            .str_err()
    } else {
        client
            .get_block_by_id(&block_id)
            .await
            .str_err()
    }
}

#[tauri::command]
pub async fn explorer_get_block_headers(
    state: State<'_, AppState>,
    count: u32,
) -> Result<Vec<serde_json::Value>, String> {
    let client = state.require_node_client().await?;
    let mut headers = client
        .get_last_block_headers_raw(count.min(100))
        .await
        .str_err()?;

    let futs: Vec<_> = headers
        .iter()
        .map(|h| {
            let id = h["id"].as_str().unwrap_or("").to_string();
            let c = client.clone();
            async move { (id.clone(), c.get_block_tx_count(&id).await.unwrap_or(0)) }
        })
        .collect();
    let counts: std::collections::HashMap<String, usize> =
        futures::future::join_all(futs).await.into_iter().collect();

    for h in &mut headers {
        if let Some(id) = h["id"].as_str().map(|s| s.to_string()) {
            if let Some(&n) = counts.get(&id) {
                h.as_object_mut()
                    .map(|obj| obj.insert("nTx".to_string(), n.into()));
            }
        }
    }

    Ok(headers)
}

#[tauri::command]
pub async fn explorer_get_mempool(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let client = state.require_node_client().await?;
    client
        .get_mempool_transactions()
        .await
        .str_err()
}

#[tauri::command]
pub async fn explorer_get_box(
    state: State<'_, AppState>,
    box_id: String,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;
    client
        .get_blockchain_box_by_id(&box_id)
        .await
        .str_err()
}

#[tauri::command]
pub async fn explorer_get_token(
    state: State<'_, AppState>,
    token_id: String,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;
    let endpoint = format!("/blockchain/token/byId/{}", token_id);
    let response = client
        .inner()
        .send_get_req(&endpoint)
        .await
        .map_err(|e| format!("Token not found: {}", e))?;
    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse token: {}", e))
}

#[tauri::command]
pub async fn explorer_get_address(
    state: State<'_, AppState>,
    address: String,
    offset: u64,
    limit: u64,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;

    let (balance_result, txs_result, unconfirmed_result) = tokio::join!(
        client.get_address_balances(&address),
        client.get_transactions_by_address(&address, offset, limit),
        client.get_unconfirmed_by_address(&address)
    );

    let (erg_balance, tokens) = balance_result.str_err()?;
    let (transactions, total_txs) = txs_result.str_err()?;
    let unconfirmed_txs = unconfirmed_result.unwrap_or_default();

    let mut unconfirmed_balance: i64 = 0;
    for utx in &unconfirmed_txs {
        if let Some(outputs) = utx["outputs"].as_array() {
            for out in outputs {
                if out["address"].as_str() == Some(&address) {
                    unconfirmed_balance += out["value"].as_i64().unwrap_or(0);
                }
            }
        }
        if let Some(inputs) = utx["inputs"].as_array() {
            for inp in inputs {
                if inp["address"].as_str() == Some(&address) {
                    unconfirmed_balance -= inp["value"].as_i64().unwrap_or(0);
                }
            }
        }
    }

    Ok(serde_json::json!({
        "address": address,
        "balance": {
            "nanoErgs": erg_balance,
            "tokens": tokens.iter().map(|(id, amt)| {
                serde_json::json!({ "tokenId": id, "amount": amt })
            }).collect::<Vec<_>>(),
        },
        "transactions": transactions,
        "totalTransactions": total_txs,
        "offset": offset,
        "limit": limit,
        "unconfirmedBalance": unconfirmed_balance,
        "unconfirmedTransactions": unconfirmed_txs,
    }))
}

#[tauri::command]
pub async fn explorer_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;
    let q = query.trim();

    if q.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(height) = q.parse::<u64>() {
            if let Ok(ids) = client.get_block_ids_at_height(height).await {
                if let Some(id) = ids.first() {
                    return Ok(serde_json::json!({
                        "type": "block",
                        "id": id,
                        "height": height,
                    }));
                }
            }
        }
    }

    if (q.starts_with('9') || q.starts_with('3'))
        && q.len() >= 40
        && client.get_address_balances(q).await.is_ok()
    {
        return Ok(serde_json::json!({
            "type": "address",
            "id": q,
        }));
    }

    if q.len() == 64 && q.chars().all(|c| c.is_ascii_hexdigit()) {
        if client.get_transaction_by_id(q).await.is_ok() {
            return Ok(serde_json::json!({
                "type": "transaction",
                "id": q,
            }));
        }
        if client.get_unconfirmed_transaction_by_id(q).await.is_ok() {
            return Ok(serde_json::json!({
                "type": "transaction",
                "id": q,
                "unconfirmed": true,
            }));
        }
        let endpoint = format!("/blockchain/token/byId/{}", q);
        if client.inner().send_get_req(&endpoint).await.is_ok() {
            return Ok(serde_json::json!({
                "type": "token",
                "id": q,
            }));
        }
        if client.get_block_by_id(q).await.is_ok() {
            return Ok(serde_json::json!({
                "type": "block",
                "id": q,
            }));
        }
    }

    Err(format!("No results found for: {}", q))
}
