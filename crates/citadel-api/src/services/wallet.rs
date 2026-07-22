//! Wallet connection, balances, transaction history, and send tx building.

use crate::dto::{
    wallet_status, ConnectionStatusResponse, RecentTxDto, RecentTxsResponse, TokenBalance,
    TokenChangeDto, WalletBalanceResponse, WalletConnectResponse, WalletStatusResponse,
};
use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use ergopay_server::RequestStatus;

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

pub async fn start_wallet_connect(state: &AppState) -> ServiceResult<WalletConnectResponse> {
    let server = state.ergopay_server().await.into_service()?;
    let (request_id, qr_url) = server.create_connect_request().await;
    let nautilus_url = server.get_nautilus_connect_url(&request_id);

    Ok(WalletConnectResponse {
        request_id,
        qr_url,
        nautilus_url,
    })
}

pub async fn get_wallet_status(state: &AppState) -> ServiceResult<WalletStatusResponse> {
    let wallet = state.wallet().await;

    Ok(WalletStatusResponse {
        connected: wallet.is_some(),
        address: wallet.as_ref().map(|w| w.address.clone()),
        addresses: wallet.map(|w| w.addresses).unwrap_or_default(),
    })
}

pub async fn get_connection_status(
    state: &AppState,
    request_id: &str,
) -> ServiceResult<ConnectionStatusResponse> {
    let server = state.ergopay_server().await.into_service()?;

    match server.get_request_status(request_id).await {
        Some(RequestStatus::Pending) => Ok(ConnectionStatusResponse {
            status: wallet_status::PENDING.to_string(),
            address: None,
            addresses: Vec::new(),
        }),
        Some(RequestStatus::AddressReceived { primary, addresses }) => {
            state
                .set_wallet_addresses(primary.clone(), addresses.clone())
                .await
                .into_service()?;

            Ok(ConnectionStatusResponse {
                status: wallet_status::CONNECTED.to_string(),
                address: Some(primary),
                addresses,
            })
        }
        Some(RequestStatus::Expired) => Ok(ConnectionStatusResponse {
            status: wallet_status::EXPIRED.to_string(),
            address: None,
            addresses: Vec::new(),
        }),
        Some(RequestStatus::Failed(msg)) => Ok(ConnectionStatusResponse {
            status: format!("{}: {}", wallet_status::FAILED, msg),
            address: None,
            addresses: Vec::new(),
        }),
        _ => Ok(ConnectionStatusResponse {
            status: "unknown".to_string(),
            address: None,
            addresses: Vec::new(),
        }),
    }
}

pub async fn disconnect_wallet(state: &AppState) -> ServiceResult<()> {
    state.disconnect_wallet().await;
    Ok(())
}

pub async fn get_wallet_balance(state: &AppState) -> ServiceResult<WalletBalanceResponse> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;
    let caps = client.require_capabilities().await?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Balance queries require extraIndex enabled on the node".to_string());
    }

    let (confirmed_erg, confirmed_tokens) = client
        .get_addresses_balances(&wallet.addresses)
        .await
        .into_service()?;

    let effective_utxos = client
        .get_effective_utxos_multi(&wallet.addresses)
        .await
        .into_service()?;
    let (erg_nano, tokens) = sum_eip12_utxos(&effective_utxos);

    let pending_erg_nano = erg_nano as i64 - confirmed_erg as i64;
    let confirmed_map: std::collections::HashMap<String, u64> =
        confirmed_tokens.into_iter().collect();

    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

    let sigusd_amount = tokens
        .iter()
        .find(|(id, _)| id == sigusd_token_id)
        .map(|(_, amt)| *amt)
        .unwrap_or(0);

    let sigrsv_amount = tokens
        .iter()
        .find(|(id, _)| id == sigrsv_token_id)
        .map(|(_, amt)| *amt)
        .unwrap_or(0);

    let mut known: std::collections::HashMap<String, (Option<String>, u8)> =
        std::collections::HashMap::new();
    known.insert(sigusd_token_id.to_string(), (Some("SigUSD".to_string()), 2));
    known.insert(sigrsv_token_id.to_string(), (Some("SigRSV".to_string()), 0));

    let mut token_balances: Vec<TokenBalance> = Vec::new();
    for (token_id, amount) in tokens {
        let (name, decimals) = if let Some(cached) = known.get(&token_id) {
            cached.clone()
        } else {
            let info = client.get_token_info(&token_id).await.ok();
            match info {
                Some(ti) => (ti.name, ti.decimals.unwrap_or(0) as u8),
                None => (None, 0u8),
            }
        };
        let pending_amount =
            amount as i64 - confirmed_map.get(&token_id).copied().unwrap_or(0) as i64;
        token_balances.push(TokenBalance {
            token_id,
            amount,
            amount_str: amount.to_string(),
            name,
            decimals,
            pending_amount,
        });
    }

    Ok(WalletBalanceResponse {
        address: wallet.address.clone(),
        addresses: wallet.addresses.clone(),
        erg_nano,
        erg_formatted: format!("{:.4}", erg_nano as f64 / 1e9),
        sigusd_amount,
        sigusd_formatted: format!("{:.2}", sigusd_amount as f64 / 100.0),
        sigrsv_amount,
        tokens: token_balances,
        pending_erg_nano,
    })
}

fn sum_eip12_utxos(utxos: &[ergo_tx::Eip12InputBox]) -> (u64, Vec<(String, u64)>) {
    let mut erg_total: u64 = 0;
    let mut token_totals: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for utxo in utxos {
        erg_total += utxo.value.parse::<u64>().unwrap_or(0);
        for asset in &utxo.assets {
            if let Ok(amount) = asset.amount.parse::<u64>() {
                *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
            }
        }
    }

    (erg_total, token_totals.into_iter().collect())
}

pub async fn get_recent_transactions(
    state: &AppState,
    limit: u64,
) -> ServiceResult<RecentTxsResponse> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;
    let caps = client.require_capabilities().await?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Transaction history requires extraIndex enabled on the node".to_string());
    }

    let address_set: std::collections::HashSet<&str> =
        wallet.addresses.iter().map(|s| s.as_str()).collect();

    let mut raw_by_id: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    for addr in &wallet.addresses {
        let batch = client
            .get_recent_transactions(addr, limit)
            .await
            .into_service()?;
        for tx in batch {
            let id = tx["id"].as_str().unwrap_or_default().to_string();
            if id.is_empty() {
                continue;
            }
            raw_by_id.entry(id).or_insert(tx);
        }
    }
    let mut raw_txs: Vec<serde_json::Value> = raw_by_id.into_values().collect();
    raw_txs.sort_by(|a, b| {
        let ta = a["timestamp"].as_u64().unwrap_or(0);
        let tb = b["timestamp"].as_u64().unwrap_or(0);
        tb.cmp(&ta)
    });
    if raw_txs.len() > limit as usize {
        raw_txs.truncate(limit as usize);
    }

    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

    let mut token_cache: std::collections::HashMap<String, (Option<String>, u8)> =
        std::collections::HashMap::new();
    token_cache.insert(sigusd_token_id.to_string(), (Some("SigUSD".to_string()), 2));
    token_cache.insert(sigrsv_token_id.to_string(), (Some("SigRSV".to_string()), 0));

    let mut transactions = Vec::new();
    for tx in &raw_txs {
        let tx_id = tx["id"].as_str().unwrap_or_default().to_string();
        let inclusion_height = tx["inclusionHeight"].as_u64().unwrap_or(0);
        let num_confirmations = tx["numConfirmations"].as_u64().unwrap_or(0);
        let timestamp = tx["timestamp"].as_u64().unwrap_or(0);

        let mut erg_in: i64 = 0;
        let mut erg_out: i64 = 0;
        let mut token_in: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut token_out: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();

        if let Some(inputs) = tx["inputs"].as_array() {
            for input in inputs {
                let addr = input["address"].as_str().unwrap_or("");
                if address_set.contains(addr) {
                    erg_in += input["value"].as_i64().unwrap_or(0);
                    if let Some(assets) = input["assets"].as_array() {
                        for asset in assets {
                            let tid = asset["tokenId"].as_str().unwrap_or_default().to_string();
                            let amt = asset["amount"].as_i64().unwrap_or(0);
                            *token_in.entry(tid).or_insert(0) += amt;
                        }
                    }
                }
            }
        }

        if let Some(outputs) = tx["outputs"].as_array() {
            for output in outputs {
                let addr = output["address"].as_str().unwrap_or("");
                if address_set.contains(addr) {
                    erg_out += output["value"].as_i64().unwrap_or(0);
                    if let Some(assets) = output["assets"].as_array() {
                        for asset in assets {
                            let tid = asset["tokenId"].as_str().unwrap_or_default().to_string();
                            let amt = asset["amount"].as_i64().unwrap_or(0);
                            *token_out.entry(tid).or_insert(0) += amt;
                        }
                    }
                }
            }
        }

        let erg_change_nano = erg_out - erg_in;

        let mut all_token_ids: std::collections::HashSet<String> =
            token_in.keys().cloned().collect();
        all_token_ids.extend(token_out.keys().cloned());

        let mut token_changes: Vec<TokenChangeDto> = Vec::new();
        for tid in all_token_ids {
            let change = token_out.get(&tid).unwrap_or(&0) - token_in.get(&tid).unwrap_or(&0);
            if change == 0 {
                continue;
            }

            let (name, decimals) = if let Some(cached) = token_cache.get(&tid) {
                cached.clone()
            } else {
                let info = client.get_token_info(&tid).await.ok();
                let resolved = match info {
                    Some(ti) => (ti.name, ti.decimals.unwrap_or(0) as u8),
                    None => (None, 0u8),
                };
                token_cache.insert(tid.clone(), resolved.clone());
                resolved
            };

            token_changes.push(TokenChangeDto {
                token_id: tid,
                amount: change,
                name,
                decimals,
            });
        }

        transactions.push(RecentTxDto {
            tx_id,
            inclusion_height,
            num_confirmations,
            timestamp,
            erg_change_nano,
            token_changes,
        });
    }

    Ok(RecentTxsResponse { transactions })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub recipient_erg: i64,
    pub token_id: Option<String>,
    pub token_amount: Option<String>,
    pub change_erg: i64,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
    pub input_count: usize,
}

pub fn build_send_tx(
    recipient_address: &str,
    change_address: &str,
    erg_nano: &str,
    token_id: Option<&str>,
    token_amount: Option<&str>,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<SendBuildResponse> {
    let send_erg: i64 = erg_nano
        .parse()
        .map_err(|e| format!("Invalid erg_nano '{}': {}", erg_nano, e))?;

    let send_token = match (token_id, token_amount) {
        (Some(tid), Some(amt)) => {
            let amount: u64 = amt
                .parse()
                .map_err(|e| format!("Invalid token_amount '{}': {}", amt, e))?;
            Some((tid, amount))
        }
        (None, None) => None,
        _ => return Err("token_id and token_amount must both be set or both omitted".to_string()),
    };

    let recipient_tree = ergo_tx::address_to_ergo_tree(recipient_address).into_service()?;
    let change_tree = ergo_tx::address_to_ergo_tree(change_address).into_service()?;

    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();

    let selected = match send_token {
        Some((tid, amount)) => {
            let with_change = (send_erg + TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO) as u64;
            match ergo_tx::select_token_boxes(&user_utxos, tid, amount, with_change) {
                Ok(sel) => sel,
                Err(_) => {
                    let exact = (send_erg + TX_FEE_NANO + citadel_fee) as u64;
                    ergo_tx::select_token_boxes(&user_utxos, tid, amount, exact).into_service()?
                }
            }
        }
        None => {
            let with_change = (send_erg + TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO) as u64;
            match ergo_tx::select_erg_boxes(&user_utxos, with_change) {
                Ok(sel) => sel,
                Err(_) => {
                    let exact = (send_erg + TX_FEE_NANO + citadel_fee) as u64;
                    ergo_tx::select_erg_boxes(&user_utxos, exact).into_service()?
                }
            }
        }
    };

    let result = ergo_tx::build_send_tx(
        &selected.boxes,
        &recipient_tree,
        &change_tree,
        send_erg,
        send_token,
        current_height,
    )
    .into_service()?;

    let unsigned_tx = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(SendBuildResponse {
        unsigned_tx,
        recipient_erg: result.summary.recipient_erg,
        token_id: result.summary.token_id,
        token_amount: result.summary.token_amount.map(|a| a.to_string()),
        change_erg: result.summary.change_erg,
        miner_fee: result.summary.miner_fee,
        citadel_fee_nano: result.summary.citadel_fee_nano,
        input_count: result.summary.input_count,
    })
}

pub async fn get_user_utxos(state: &AppState) -> ServiceResult<Vec<ergo_tx::Eip12InputBox>> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;

    client
        .get_effective_utxos_multi(&wallet.addresses)
        .await
        .into_service()
}

pub fn validate_ergo_address(address: &str) -> ServiceResult<String> {
    ergo_tx::address_to_ergo_tree(address).into_service()
}

#[cfg(test)]
mod tests {
    use super::sum_eip12_utxos;
    use ergo_tx::{Eip12Asset, Eip12InputBox};
    use std::collections::HashMap;

    fn make_box(value: &str, assets: Vec<(&str, &str)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "b".to_string(),
            transaction_id: "t".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd".to_string(),
            assets: assets
                .into_iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: id.to_string(),
                    amount: amt.to_string(),
                })
                .collect(),
            creation_height: 1,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn sums_erg_and_tokens_across_boxes() {
        let utxos = vec![
            make_box("1000000000", vec![("tok_a", "5")]),
            make_box("500000000", vec![("tok_a", "3"), ("tok_b", "7")]),
        ];
        let (erg, tokens) = sum_eip12_utxos(&utxos);
        assert_eq!(erg, 1_500_000_000);
        let map: HashMap<String, u64> = tokens.into_iter().collect();
        assert_eq!(map["tok_a"], 8);
        assert_eq!(map["tok_b"], 7);
    }

    #[test]
    fn pending_delta_math() {
        let effective = vec![make_box("900000000", vec![("tok_new", "42")])];
        let (erg, tokens) = sum_eip12_utxos(&effective);
        let confirmed_erg: u64 = 1_000_000_000;
        let confirmed: HashMap<String, u64> = HashMap::new();

        let pending_erg = erg as i64 - confirmed_erg as i64;
        assert_eq!(pending_erg, -100_000_000);

        let map: HashMap<String, u64> = tokens.into_iter().collect();
        let pending_tok =
            map["tok_new"] as i64 - confirmed.get("tok_new").copied().unwrap_or(0) as i64;
        assert_eq!(pending_tok, 42);
    }

    #[test]
    fn unparseable_values_are_skipped() {
        let utxos = vec![make_box("not-a-number", vec![("tok_a", "bad")])];
        let (erg, tokens) = sum_eip12_utxos(&utxos);
        assert_eq!(erg, 0);
        assert!(tokens.is_empty());
    }
}
