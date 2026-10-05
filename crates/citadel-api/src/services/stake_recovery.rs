//! Stake recovery use-case orchestration: multi-protocol stake scan, recovery tx
//! building, and the Paideia permissionless proxy check/submit flow.

use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use serde::Serialize;
use stake_recovery::{
    build_paideia_executor_tx, build_paideia_proxy_tx, build_paideia_refund_tx,
    build_recovery_tx_eip12, discover_pending_paideia_proxies, discover_recoverable_stakes,
    fetch_stake_box_by_key, fetch_stake_state, find_stake_box_by_key, parse_stake_box,
    RecoverableStake, RecoveryMechanism, RecoveryScan, PAIDEIA, PAIDEIA_PROXY_ERGO_TREE,
};

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

/// Scan every registered v1 staking protocol (Ergopad, EGIO, …) for StakeBoxes whose
/// R5 matches any of the provided candidate token IDs (typically the wallet's
/// unique-qty-1 tokens). Auto-detects which protocol each recovered stake belongs to.
pub async fn scan_recoverable_stakes(
    state: &AppState,
    candidate_token_ids: &[String],
) -> ServiceResult<RecoveryScan> {
    let client = state.require_node_client().await?;
    discover_recoverable_stakes(&client, candidate_token_ids)
        .await
        .into_service()
}

/// Expose the parsed stake for a given key (used by the UI to render confirm
/// dialogs). Auto-detects the owning protocol.
pub async fn preview_recovery(
    state: &AppState,
    stake_key_id: &str,
) -> ServiceResult<RecoverableStake> {
    let client = state.require_node_client().await?;
    let (cfg, stake_ergo_box) = find_stake_box_by_key(&client, stake_key_id)
        .await
        .into_service()?;
    parse_stake_box(&stake_ergo_box, cfg).into_service()
}

/// Build the 3-input recovery tx for a single stake key. The owning protocol is
/// auto-detected from the key. `user_utxos` must include the box holding the stake
/// key NFT.
pub async fn build_recovery_tx(
    state: &AppState,
    stake_key_id: &str,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;

    // Detect the protocol by locating the live StakeBox for this key.
    let (cfg, stake_ergo_box) = find_stake_box_by_key(&client, stake_key_id)
        .await
        .into_service()?;
    let stake = parse_stake_box(&stake_ergo_box, cfg).into_service()?;

    // Dispatch on the pool's recovery mechanism. Paideia is redeemed via a proxy box
    // (step 1 of 2) rather than the Ergopad/EGIO/NETA direct unstake; the two tx shapes are
    // not interchangeable.
    match cfg.mechanism {
        RecoveryMechanism::Direct | RecoveryMechanism::NetaDirect => {
            let (state_ergo_box, state_snapshot) =
                fetch_stake_state(&client, cfg).await.into_service()?;
            let state_box = ergo_box_to_eip12(&state_ergo_box);
            let stake_box = ergo_box_to_eip12(&stake_ergo_box);
            let user_ergo_tree = recipient_ergo_tree_for_key(&user_utxos, &stake.stake_key_id)?;

            build_recovery_tx_eip12(
                &state_box,
                &state_snapshot,
                &stake_box,
                &stake,
                &user_utxos,
                &user_ergo_tree,
                current_height,
            )
            .into_service()
        }
        RecoveryMechanism::PaideiaProxy => {
            // Payout recipient = the wallet address that holds the stake key (a P2PK).
            let recipient_ergo_tree =
                recipient_ergo_tree_for_key(&user_utxos, &stake.stake_key_id)?;
            let (_state_ergo_box, state_snapshot) =
                fetch_stake_state(&client, cfg).await.into_service()?;

            build_paideia_proxy_tx(
                &stake,
                &state_snapshot,
                &user_utxos,
                &recipient_ergo_tree,
                current_height,
            )
            .into_service()
        }
    }
}

/// Select the ErgoTree of the parsed UTXO that actually carries `stake_key_id` — the
/// recipient of a recovery must be whoever holds the key, not just whichever UTXO
/// happens to be first in the (arbitrarily ordered) wallet UTXO list.
fn recipient_ergo_tree_for_key(
    parsed_utxos: &[ergo_tx::Eip12InputBox],
    stake_key_id: &str,
) -> ServiceResult<String> {
    parsed_utxos
        .iter()
        .find(|b| b.assets.iter().any(|a| a.token_id == stake_key_id))
        .map(|b| b.ergo_tree.clone())
        .ok_or_else(|| format!("Stake key {} not found in provided utxos", stake_key_id))
}

// ErgoBox already contains its creation transaction and output index, including
// boxes created in the mempool. A confirmed-only index lookup breaks 0-conf use.
fn ergo_box_to_eip12(ergo_box: &ErgoBox) -> ergo_tx::Eip12InputBox {
    ergo_tx::Eip12InputBox::from_ergo_box(
        ergo_box,
        ergo_box.transaction_id.to_string(),
        ergo_box.index,
    )
}

/// Outcome of a `POST /transactions/check` dry-run for one leg of the Paideia flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunResult {
    /// True if the node would accept this exact transaction against the live UTXO set.
    pub valid: bool,
    /// The node's accepted tx id on success, or its rejection reason on failure.
    pub message: String,
}

/// Combined dry-run report for the two permissionless Paideia proxy spend paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaideiaProxyCheck {
    /// The proxy box being checked.
    pub proxy_box_id: String,
    /// The reward-payout (unstake) path — the tx that pays the reward and burns the key.
    pub executor: DryRunResult,
    /// The refund path — returns the key + ERG to the recipient if the unstake can't run.
    pub refund: DryRunResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPaideiaRecovery {
    pub proxy_box_id: String,
    pub creation_tx_id: String,
    pub stake_key_id: String,
    pub recipient_address: String,
    pub reward_amount_display: String,
    pub confirmed: bool,
    pub creation_height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPaideiaScan {
    pub recoveries: Vec<PendingPaideiaRecovery>,
    pub boxes_scanned: u64,
    pub pages_fetched: u64,
    pub hit_page_limit: bool,
}

/// Find all outstanding step-one proxies belonging to the connected wallet,
/// including proxies created in old transactions or in the node's mempool.
pub async fn scan_pending_paideia_recoveries(
    state: &AppState,
) -> ServiceResult<PendingPaideiaScan> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "Connect a wallet to find unfinished Paideia recoveries".to_string())?;
    let recipients: Vec<(String, String)> = wallet
        .addresses
        .iter()
        .map(|address| {
            ergo_node_client::address_to_ergo_tree(address)
                .map(|tree| (tree, address.clone()))
                .ok_or_else(|| format!("Cannot read wallet address {address}"))
        })
        .collect::<ServiceResult<_>>()?;
    let trees: Vec<String> = recipients.iter().map(|(tree, _)| tree.clone()).collect();
    let client = state.require_node_client().await?;
    let scan = discover_pending_paideia_proxies(&client, &trees)
        .await
        .into_service()?;
    let recoveries = scan
        .candidates
        .into_iter()
        .map(|candidate| {
            let b = candidate.proxy_box;
            let amount = candidate.reward_amount_raw;
            Ok(PendingPaideiaRecovery {
                proxy_box_id: b.box_id().to_string(),
                creation_tx_id: b.transaction_id.to_string(),
                stake_key_id: candidate.stake_key_id,
                recipient_address: recipients
                    .iter()
                    .find(|(tree, _)| tree == &candidate.recipient_ergo_tree)
                    .map(|(_, address)| address.clone())
                    .ok_or_else(|| {
                        "Proxy recipient does not belong to the connected wallet".to_string()
                    })?,
                reward_amount_display: format!("{}.{:04}", amount / 10_000, amount % 10_000),
                confirmed: candidate.confirmed,
                creation_height: b.creation_height,
            })
        })
        .collect::<ServiceResult<Vec<_>>>()?;
    Ok(PendingPaideiaScan {
        recoveries,
        boxes_scanned: scan.boxes_scanned,
        pages_fetched: scan.pages_fetched,
        hit_page_limit: scan.hit_page_limit,
    })
}

/// Convert a built (empty-proof) EIP-12 tx into the node's `ErgoTransaction` JSON for
/// `/transactions/check` and `/transactions`. The Paideia unstake and refund require no
/// signatures, so every input carries an empty spending proof.
fn eip12_tx_to_node_json(tx: &ergo_tx::Eip12UnsignedTx) -> ServiceResult<serde_json::Value> {
    let inputs: Vec<serde_json::Value> = tx
        .inputs
        .iter()
        .map(|i| {
            serde_json::json!({
                "boxId": i.box_id,
                "spendingProof": { "proofBytes": "", "extension": {} }
            })
        })
        .collect();

    let mut outputs = Vec::with_capacity(tx.outputs.len());
    for o in &tx.outputs {
        let value: u64 = o
            .value
            .parse()
            .map_err(|_| format!("Invalid output value '{}'", o.value))?;
        let mut assets = Vec::with_capacity(o.assets.len());
        for a in &o.assets {
            let amount: u64 = a
                .amount
                .parse()
                .map_err(|_| format!("Invalid asset amount '{}'", a.amount))?;
            assets.push(serde_json::json!({ "tokenId": a.token_id, "amount": amount }));
        }
        outputs.push(serde_json::json!({
            "value": value,
            "ergoTree": o.ergo_tree,
            "assets": assets,
            "creationHeight": o.creation_height,
            "additionalRegisters": o.additional_registers,
        }));
    }

    Ok(serde_json::json!({
        "inputs": inputs,
        "dataInputs": [],
        "outputs": outputs,
    }))
}

/// Given a live Paideia unstake proxy box (created by step 1), rebuild
/// both permissionless spend paths and assemble them: the executor (reward payout) tx
/// and the refund tx. Returns the two node-format txs plus the resolved metadata.
async fn assemble_paideia_proxy_txs(
    client: &ergo_node_client::NodeClient,
    proxy_box_id: &str,
) -> ServiceResult<(ServiceResult<serde_json::Value>, serde_json::Value, String)> {
    let proxy_box = client
        .get_eip12_box_by_id(proxy_box_id)
        .await
        .map_err(|e| format!("Failed to fetch proxy box {}: {}", proxy_box_id, e))?;
    if proxy_box.ergo_tree != PAIDEIA_PROXY_ERGO_TREE {
        return Err(format!(
            "Box {} is not a Paideia unstake proxy box",
            proxy_box_id
        ));
    }
    let stake_key_id = proxy_box
        .assets
        .first()
        .map(|a| a.token_id.clone())
        .ok_or_else(|| "Proxy box carries no stake key NFT".to_string())?;

    let current_height = client
        .current_height()
        .await
        .map_err(|e| format!("Failed to read height: {}", e))? as i32;

    // Refund only needs the proxy box itself.
    let refund_tx = build_paideia_refund_tx(&proxy_box, current_height).into_service()?;
    let refund_json = eip12_tx_to_node_json(&refund_tx)?;

    // A missing or changed stake can invalidate payout while the key refund is
    // still available. Keep its assembly result independent of the refund.
    let executor_json =
        assemble_paideia_executor_tx(client, &proxy_box, &stake_key_id, current_height).await;
    Ok((executor_json, refund_json, proxy_box.box_id.clone()))
}

async fn assemble_paideia_executor_tx(
    client: &ergo_node_client::NodeClient,
    proxy_box: &ergo_tx::Eip12InputBox,
    stake_key_id: &str,
    current_height: i32,
) -> ServiceResult<serde_json::Value> {
    // Executor needs the live StakeStateBox and the matching StakeBox. Query the
    // Paideia StakeBox directly rather than the generic multi-protocol
    // `find_stake_box_by_key` — this key came out of a real Paideia proxy box, so
    // its StakeBox must be Paideia's; using the auto-detector here would let it
    // return a mismatched `cfg` (a decoy/foreign box at a different protocol's
    // address) instead of erroring, silently pairing this box's data with the
    // wrong protocol's `parse_stake_box`.
    let (state_ergo_box, state_snapshot) =
        fetch_stake_state(client, &PAIDEIA).await.into_service()?;
    let stake_ergo_box = fetch_stake_box_by_key(client, &PAIDEIA, stake_key_id)
        .await
        .into_service()?;
    let stake = parse_stake_box(&stake_ergo_box, &PAIDEIA).into_service()?;
    let state_box = ergo_box_to_eip12(&state_ergo_box);
    let stake_box = ergo_box_to_eip12(&stake_ergo_box);

    // Pay the executor tip (OUTPUTS[3], script-free) to the proxy's own R5 recipient so
    // the key-holder collects it when they self-execute the payout.
    let executor_tx = build_paideia_executor_tx(
        &state_box,
        &state_snapshot,
        &stake_box,
        &stake,
        proxy_box,
        &recipient_tree_from_proxy(proxy_box)?,
        current_height,
    )
    .into_service()?;
    eip12_tx_to_node_json(&executor_tx)
}

/// Read the payout recipient's ErgoTree (hex) from a proxy box's R5 register.
fn recipient_tree_from_proxy(proxy_box: &ergo_tx::Eip12InputBox) -> ServiceResult<String> {
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let hex_val = proxy_box
        .additional_registers
        .get("R5")
        .ok_or_else(|| "Proxy box missing R5".to_string())?;
    let raw = hex::decode(hex_val.trim()).map_err(|e| format!("R5 not hex: {}", e))?;
    let constant = Constant::sigma_parse_bytes(&raw).map_err(|e| format!("R5 parse: {}", e))?;
    match &constant.v {
        Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes))) => Ok(hex::encode(
            bytes.iter().map(|&x| x as u8).collect::<Vec<u8>>(),
        )),
        other => Err(format!("R5 is not Coll[Byte]: {:?}", other)),
    }
}

/// Resolve the unstake proxy box (OUTPUT[0]) created by step one. Unconfirmed
/// transactions are available as soon as they reach the connected node's mempool.
pub async fn paideia_proxy_box_id(state: &AppState, tx_id: &str) -> ServiceResult<String> {
    let client = state.require_node_client().await?;
    let tx = match client.get_unconfirmed_transaction_by_id(tx_id).await {
        Ok(tx)
            if tx
                .get("outputs")
                .and_then(|outputs| outputs.as_array())
                .is_some() =>
        {
            tx
        }
        _ => client.get_transaction_by_id(tx_id).await.map_err(|e| {
            format!("Step-one transaction {tx_id} is not available on the connected node yet: {e}")
        })?,
    };
    let out0 = tx
        .get("outputs")
        .and_then(|o| o.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "Step-1 tx has no outputs".to_string())?;
    let tree = out0.get("ergoTree").and_then(|t| t.as_str()).unwrap_or("");
    if tree != PAIDEIA_PROXY_ERGO_TREE {
        return Err("Step-1 tx output[0] is not a Paideia unstake proxy box".to_string());
    }
    out0.get("boxId")
        .and_then(|b| b.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Proxy output missing boxId".to_string())
}

/// Dry-run BOTH permissionless spend paths of a live Paideia proxy box through the
/// node's `/transactions/check` without broadcasting. This is the execution-time safety
/// gate: once step one is in the node's mempool, the UI shows whether the payout and refund
/// both validate against the *exact* proxy box step 1 created, before the user commits to
/// either. Neither tx needs a signature.
pub async fn check_paideia_proxy(
    state: &AppState,
    proxy_box_id: &str,
) -> ServiceResult<PaideiaProxyCheck> {
    let client = state.require_node_client().await?;
    let (executor_json, refund_json, box_id) =
        assemble_paideia_proxy_txs(&client, proxy_box_id).await?;

    let executor_check = match executor_json {
        Ok(tx) => client
            .check_transaction(&tx)
            .await
            .map_err(|e| e.to_string()),
        Err(error) => Err(error),
    };
    let executor = match executor_check {
        Ok(tx_id) => DryRunResult {
            valid: true,
            message: tx_id,
        },
        Err(e) => DryRunResult {
            valid: false,
            message: e.to_string(),
        },
    };
    let refund = match client.check_transaction(&refund_json).await {
        Ok(tx_id) => DryRunResult {
            valid: true,
            message: tx_id,
        },
        Err(e) => DryRunResult {
            valid: false,
            message: e.to_string(),
        },
    };

    Ok(PaideiaProxyCheck {
        proxy_box_id: box_id,
        executor,
        refund,
    })
}

/// Broadcast one permissionless Paideia proxy spend path. `which` is `"executor"` (pay
/// out the reward, burn the key) or `"refund"` (return the key + ERG to the recipient).
/// The transaction is dry-run through `/transactions/check` first; broadcast is aborted
/// if the node would reject it. No signature is involved — the button press is the user's
/// authorization.
pub async fn submit_paideia_proxy_tx(
    state: &AppState,
    proxy_box_id: &str,
    which: &str,
) -> ServiceResult<String> {
    let client = state.require_node_client().await?;
    let (executor_json, refund_json, _box_id) =
        assemble_paideia_proxy_txs(&client, proxy_box_id).await?;

    let tx_json = match which {
        "executor" => executor_json?,
        "refund" => refund_json,
        other => return Err(format!("Unknown proxy path '{}'", other)),
    };

    // Never broadcast a tx the node would reject.
    client
        .check_transaction(&tx_json)
        .await
        .map_err(|e| format!("Pre-broadcast check failed, not submitting: {}", e))?;

    client.submit_transaction(&tx_json).await.into_service()
}
