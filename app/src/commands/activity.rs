use citadel_api::AppState;
use dexy::{
    constants::{DexyIds, DexyVariant},
    fetch::fetch_dexy_state,
};
use ergo_node_client::NodeClient;
use serde::Serialize;
use sigmausd::{fetch_sigmausd_state, NftIds};
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolInteraction {
    pub tx_id: String,
    pub height: u64,
    pub timestamp: u64,
    pub protocol: String,
    pub operation: String,
    pub token: String,
    pub erg_change_nano: i64,
    pub token_amount_change: i64,
}

/// Trace a bank NFT backwards through the chain to find recent interactions.
///
/// Starting from the current bank box, we follow the chain of transactions
/// backwards: each bank box was created by a transaction whose input contained
/// the previous bank box. By comparing the ERG and token values between
/// successive bank boxes, we can classify each interaction as mint or redeem.
async fn trace_bank_nft(
    client: &NodeClient,
    bank_box_id: &str,
    bank_nft_id: &str,
    protocol: &str,
    token_ids: &[(&str, &str)], // [(token_id, token_name), ...]
    count: usize,
) -> Vec<ProtocolInteraction> {
    let mut results = Vec::new();
    let mut current_box_id = bank_box_id.to_string();

    for _ in 0..count {
        // Get the current bank box to find which tx created it
        let current_box = match client.get_blockchain_box_by_id(&current_box_id).await {
            Ok(b) => b,
            Err(_) => break,
        };

        let tx_id = match current_box["transactionId"].as_str() {
            Some(id) => id.to_string(),
            None => break,
        };

        let current_value = current_box["value"].as_i64().unwrap_or(0);
        let current_height = current_box["settlementHeight"]
            .as_u64()
            .or_else(|| current_box["creationHeight"].as_u64())
            .unwrap_or(0);

        // Get token amounts in current box
        let current_tokens: Vec<(String, i64)> = current_box["assets"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let tid = t["tokenId"].as_str()?;
                        let amt = t["amount"].as_i64()?;
                        Some((tid.to_string(), amt))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Get the transaction to find the previous bank box (in inputs)
        let tx = match client.get_transaction_by_id(&tx_id).await {
            Ok(t) => t,
            Err(_) => break,
        };

        let timestamp = tx["timestamp"].as_u64().unwrap_or(0);
        let height = tx["inclusionHeight"].as_u64().unwrap_or(current_height);

        // Find which input had the bank NFT by checking each input box
        let mut found_prev_box: Option<serde_json::Value> = None;
        let mut found_prev_box_id: Option<String> = None;

        if let Some(inputs) = tx["inputs"].as_array() {
            for input in inputs {
                if let Some(input_box_id) = input["boxId"].as_str() {
                    // Skip if this is the current box (shouldn't be an input)
                    if input_box_id == current_box_id {
                        continue;
                    }
                    if let Ok(input_box) = client.get_blockchain_box_by_id(input_box_id).await {
                        let has_nft = input_box["assets"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .any(|t| t["tokenId"].as_str() == Some(bank_nft_id))
                            })
                            .unwrap_or(false);
                        if has_nft {
                            found_prev_box_id = Some(input_box_id.to_string());
                            found_prev_box = Some(input_box);
                            break;
                        }
                    }
                }
            }
        }

        let prev_box = match found_prev_box {
            Some(b) => b,
            None => break,
        };

        let prev_value = prev_box["value"].as_i64().unwrap_or(0);
        let prev_tokens: Vec<(String, i64)> = prev_box["assets"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let tid = t["tokenId"].as_str()?;
                        let amt = t["amount"].as_i64()?;
                        Some((tid.to_string(), amt))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Compare old vs new bank box to classify the interaction
        let erg_change = current_value - prev_value;

        // Check each tracked token for changes
        for (token_id, token_name) in token_ids {
            let prev_amt = prev_tokens
                .iter()
                .find(|(id, _)| id == token_id)
                .map(|(_, a)| *a)
                .unwrap_or(0);
            let curr_amt = current_tokens
                .iter()
                .find(|(id, _)| id == token_id)
                .map(|(_, a)| *a)
                .unwrap_or(0);
            let token_change = curr_amt - prev_amt;

            if token_change != 0 {
                // Token count increased in bank = user redeemed (returned tokens)
                // Token count decreased in bank = user minted (took tokens)
                let operation = if token_change > 0 { "redeem" } else { "mint" };
                results.push(ProtocolInteraction {
                    tx_id: tx_id.clone(),
                    height,
                    timestamp,
                    protocol: protocol.to_string(),
                    operation: operation.to_string(),
                    token: token_name.to_string(),
                    erg_change_nano: erg_change,
                    token_amount_change: token_change.unsigned_abs() as i64,
                });
                break; // One interaction per tx
            }
        }

        // If no token change was detected but ERG changed, record it as generic
        if results.last().map(|r| &r.tx_id) != Some(&tx_id) && erg_change != 0 {
            let operation = if erg_change > 0 { "mint" } else { "redeem" };
            results.push(ProtocolInteraction {
                tx_id: tx_id.clone(),
                height,
                timestamp,
                protocol: protocol.to_string(),
                operation: operation.to_string(),
                token: token_ids
                    .first()
                    .map(|(_, n)| n.to_string())
                    .unwrap_or_default(),
                erg_change_nano: erg_change,
                token_amount_change: 0,
            });
        }

        // Move to the previous bank box for the next iteration
        current_box_id = match found_prev_box_id {
            Some(id) => id,
            None => break,
        };
    }

    results
}

/// Get recent protocol interactions by tracing bank NFTs
#[tauri::command]
pub async fn get_protocol_activity(
    state: State<'_, AppState>,
    count: u64,
) -> Result<Vec<ProtocolInteraction>, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;

    // Get SigmaUSD state
    let nft_ids =
        NftIds::for_network(config.network).ok_or_else(|| "SigmaUSD not available".to_string())?;
    let sigma_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| format!("Failed to fetch SigmaUSD state: {}", e))?;

    // Get Dexy states
    let dexy_gold_ids = DexyIds::for_variant(DexyVariant::Gold, config.network);
    let dexy_usd_ids = DexyIds::for_variant(DexyVariant::Usd, config.network);

    let count = count as usize;

    // Trace SigmaUSD bank NFT
    let sigma_fut = {
        let client = &client;
        let bank_box_id = sigma_state.bank_box_id.clone();
        let bank_nft = nft_ids.bank_nft.clone();
        let sigusd_token = nft_ids.sigusd_token.clone();
        let sigrsv_token = nft_ids.sigrsv_token.clone();
        async move {
            let token_ids: Vec<(&str, &str)> =
                vec![(&sigusd_token, "SigUSD"), (&sigrsv_token, "SigRSV")];
            trace_bank_nft(
                client,
                &bank_box_id,
                &bank_nft,
                "SigmaUSD",
                &token_ids,
                count,
            )
            .await
        }
    };

    // Trace Dexy Gold bank NFT
    let dexy_gold_fut = async {
        if let Some(ids) = &dexy_gold_ids {
            let dexy_state = match fetch_dexy_state(&client, &capabilities, ids).await {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let token_ids: Vec<(&str, &str)> = vec![(&ids.dexy_token, "DexyGold")];
            trace_bank_nft(
                &client,
                &dexy_state.bank_box_id,
                &ids.bank_nft,
                "DexyGold",
                &token_ids,
                count,
            )
            .await
        } else {
            Vec::new()
        }
    };

    // Trace Dexy USD bank NFT
    let dexy_usd_fut = async {
        if let Some(ids) = &dexy_usd_ids {
            let dexy_state = match fetch_dexy_state(&client, &capabilities, ids).await {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let token_ids: Vec<(&str, &str)> = vec![(&ids.dexy_token, "USE")];
            trace_bank_nft(
                &client,
                &dexy_state.bank_box_id,
                &ids.bank_nft,
                "DexyUSD",
                &token_ids,
                count,
            )
            .await
        } else {
            Vec::new()
        }
    };

    // Run all traces concurrently
    let (sigma_activity, dexy_gold_activity, dexy_usd_activity) =
        tokio::join!(sigma_fut, dexy_gold_fut, dexy_usd_fut);

    // Merge and sort by height descending
    let mut all: Vec<ProtocolInteraction> = Vec::new();
    all.extend(sigma_activity);
    all.extend(dexy_gold_activity);
    all.extend(dexy_usd_activity);
    all.sort_by(|a, b| b.height.cmp(&a.height));
    all.truncate(count);

    Ok(all)
}

/// Get recent Dexy-only protocol interactions by tracing DexyGold and DexyUSD bank NFTs
#[tauri::command]
pub async fn get_dexy_activity(
    state: State<'_, AppState>,
    count: u64,
) -> Result<Vec<ProtocolInteraction>, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;

    let dexy_gold_ids = DexyIds::for_variant(DexyVariant::Gold, config.network);
    let dexy_usd_ids = DexyIds::for_variant(DexyVariant::Usd, config.network);

    let count = count as usize;

    let dexy_gold_fut = async {
        if let Some(ids) = &dexy_gold_ids {
            let dexy_state = match fetch_dexy_state(&client, &capabilities, ids).await {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let token_ids: Vec<(&str, &str)> = vec![(&ids.dexy_token, "DexyGold")];
            trace_bank_nft(
                &client,
                &dexy_state.bank_box_id,
                &ids.bank_nft,
                "DexyGold",
                &token_ids,
                count,
            )
            .await
        } else {
            Vec::new()
        }
    };

    let dexy_usd_fut = async {
        if let Some(ids) = &dexy_usd_ids {
            let dexy_state = match fetch_dexy_state(&client, &capabilities, ids).await {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let token_ids: Vec<(&str, &str)> = vec![(&ids.dexy_token, "USE")];
            trace_bank_nft(
                &client,
                &dexy_state.bank_box_id,
                &ids.bank_nft,
                "DexyUSD",
                &token_ids,
                count,
            )
            .await
        } else {
            Vec::new()
        }
    };

    let (dexy_gold_activity, dexy_usd_activity) = tokio::join!(dexy_gold_fut, dexy_usd_fut);

    let mut all: Vec<ProtocolInteraction> = Vec::new();
    all.extend(dexy_gold_activity);
    all.extend(dexy_usd_activity);
    all.sort_by(|a, b| b.height.cmp(&a.height));
    all.truncate(count);

    Ok(all)
}

/// Get recent SigmaUSD-only protocol interactions by tracing the SigmaUSD bank NFT
#[tauri::command]
pub async fn get_sigmausd_activity(
    state: State<'_, AppState>,
    count: u64,
) -> Result<Vec<ProtocolInteraction>, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;

    let nft_ids =
        NftIds::for_network(config.network).ok_or_else(|| "SigmaUSD not available".to_string())?;
    let sigma_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| format!("Failed to fetch SigmaUSD state: {}", e))?;

    let count = count as usize;

    let token_ids: Vec<(&str, &str)> = vec![
        (&nft_ids.sigusd_token as &str, "SigUSD"),
        (&nft_ids.sigrsv_token as &str, "SigRSV"),
    ];
    let mut results = trace_bank_nft(
        &client,
        &sigma_state.bank_box_id,
        &nft_ids.bank_nft,
        "SigmaUSD",
        &token_ids,
        count,
    )
    .await;

    results.sort_by(|a, b| b.height.cmp(&a.height));
    results.truncate(count);

    Ok(results)
}
