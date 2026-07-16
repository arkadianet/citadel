use citadel_api::AppState;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use serde::Serialize;
use stake_recovery::{
    build_paideia_executor_tx, build_paideia_proxy_tx, build_paideia_refund_tx,
    build_recovery_tx_eip12, discover_recoverable_stakes, fetch_stake_box_by_key,
    fetch_stake_state, find_stake_box_by_key, parse_stake_box, RecoverableStake, RecoveryMechanism,
    RecoveryScan, PAIDEIA, PAIDEIA_PROXY_ERGO_TREE,
};
use tauri::State;

use super::StrErr;

/// Scan every registered v1 staking protocol (Ergopad, EGIO, …) for StakeBoxes whose
/// R5 matches any of the provided candidate token IDs (typically the wallet's
/// unique-qty-1 tokens). Auto-detects which protocol each recovered stake belongs to.
#[tauri::command]
pub async fn scan_recoverable_stakes(
    state: State<'_, AppState>,
    candidate_token_ids: Vec<String>,
) -> Result<RecoveryScan, String> {
    let client = state.require_node_client().await?;
    discover_recoverable_stakes(&client, &candidate_token_ids)
        .await
        .str_err()
}

/// Build the 3-input recovery tx for a single stake key. The owning protocol is
/// auto-detected from the key. `user_utxos` must include the box holding the stake
/// key NFT.
#[tauri::command]
pub async fn build_recovery_tx(
    state: State<'_, AppState>,
    stake_key_id: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;

    // Detect the protocol by locating the live StakeBox for this key.
    let (cfg, stake_ergo_box) = find_stake_box_by_key(&client, &stake_key_id)
        .await
        .str_err()?;
    let stake = parse_stake_box(&stake_ergo_box, cfg).str_err()?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    // Dispatch on the pool's recovery mechanism. Paideia is redeemed via a proxy box
    // (step 1 of 2) rather than the Ergopad/EGIO direct unstake; the two tx shapes are
    // not interchangeable.
    match cfg.mechanism {
        RecoveryMechanism::Direct => {
            let (state_ergo_box, state_snapshot) =
                fetch_stake_state(&client, cfg).await.str_err()?;
            let state_box = ergo_box_to_eip12(&client, &state_ergo_box).await?;
            let stake_box = ergo_box_to_eip12(&client, &stake_ergo_box).await?;
            let user_ergo_tree = recipient_ergo_tree_for_key(&parsed_utxos, &stake.stake_key_id)?;

            let unsigned_tx = build_recovery_tx_eip12(
                &state_box,
                &state_snapshot,
                &stake_box,
                &stake,
                &parsed_utxos,
                &user_ergo_tree,
                current_height,
            )
            .str_err()?;
            serde_json::to_value(&unsigned_tx)
                .map_err(|e| format!("Failed to serialize transaction: {}", e))
        }
        RecoveryMechanism::PaideiaProxy => {
            // Payout recipient = the wallet address that holds the stake key (a P2PK).
            let recipient_ergo_tree =
                recipient_ergo_tree_for_key(&parsed_utxos, &stake.stake_key_id)?;
            let (_state_ergo_box, state_snapshot) =
                fetch_stake_state(&client, cfg).await.str_err()?;

            let unsigned_tx = build_paideia_proxy_tx(
                &stake,
                &state_snapshot,
                &parsed_utxos,
                &recipient_ergo_tree,
                current_height,
            )
            .str_err()?;
            serde_json::to_value(&unsigned_tx)
                .map_err(|e| format!("Failed to serialize transaction: {}", e))
        }
    }
}

/// Select the ErgoTree of the parsed UTXO that actually carries `stake_key_id` — the
/// recipient of a recovery must be whoever holds the key, not just whichever UTXO
/// happens to be first in the (arbitrarily ordered) wallet UTXO list.
fn recipient_ergo_tree_for_key(
    parsed_utxos: &[ergo_tx::Eip12InputBox],
    stake_key_id: &str,
) -> Result<String, String> {
    parsed_utxos
        .iter()
        .find(|b| b.assets.iter().any(|a| a.token_id == stake_key_id))
        .map(|b| b.ergo_tree.clone())
        .ok_or_else(|| format!("Stake key {} not found in provided utxos", stake_key_id))
}

/// Expose the parsed stake for a given key (used by the UI to render confirm
/// dialogs). Auto-detects the owning protocol.
#[tauri::command]
pub async fn preview_recovery(
    state: State<'_, AppState>,
    stake_key_id: String,
) -> Result<RecoverableStake, String> {
    let client = state.require_node_client().await?;
    let (cfg, stake_ergo_box) = find_stake_box_by_key(&client, &stake_key_id)
        .await
        .str_err()?;
    parse_stake_box(&stake_ergo_box, cfg).str_err()
}

async fn ergo_box_to_eip12(
    client: &ergo_node_client::NodeClient,
    ergo_box: &ErgoBox,
) -> Result<ergo_tx::Eip12InputBox, String> {
    let box_id = hex::encode(ergo_box.box_id().as_ref());
    let (tx_id, index) = client
        .get_box_creation_info(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch box context for {}: {}", box_id, e))?;
    Ok(ergo_tx::Eip12InputBox::from_ergo_box(
        ergo_box, tx_id, index,
    ))
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

/// Convert a built (empty-proof) EIP-12 tx into the node's `ErgoTransaction` JSON for
/// `/transactions/check` and `/transactions`. The Paideia unstake and refund require no
/// signatures, so every input carries an empty spending proof.
fn eip12_tx_to_node_json(tx: &ergo_tx::Eip12UnsignedTx) -> Result<serde_json::Value, String> {
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

/// Given the ID of a confirmed Paideia unstake proxy box (created by step 1), rebuild
/// both permissionless spend paths and assemble them: the executor (reward payout) tx
/// and the refund tx. Returns the two node-format txs plus the resolved metadata.
async fn assemble_paideia_proxy_txs(
    client: &ergo_node_client::NodeClient,
    proxy_box_id: &str,
) -> Result<(serde_json::Value, serde_json::Value, String), String> {
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
    let refund_tx = build_paideia_refund_tx(&proxy_box, current_height).str_err()?;
    let refund_json = eip12_tx_to_node_json(&refund_tx)?;

    // Executor needs the live StakeStateBox and the matching StakeBox. Query the
    // Paideia StakeBox directly rather than the generic multi-protocol
    // `find_stake_box_by_key` — this key came out of a real Paideia proxy box, so
    // its StakeBox must be Paideia's; using the auto-detector here would let it
    // return a mismatched `cfg` (a decoy/foreign box at a different protocol's
    // address) instead of erroring, silently pairing this box's data with the
    // wrong protocol's `parse_stake_box`.
    let (state_ergo_box, state_snapshot) = fetch_stake_state(client, &PAIDEIA).await.str_err()?;
    let stake_ergo_box = fetch_stake_box_by_key(client, &PAIDEIA, &stake_key_id)
        .await
        .str_err()?;
    let stake = parse_stake_box(&stake_ergo_box, &PAIDEIA).str_err()?;
    let state_box = ergo_box_to_eip12(client, &state_ergo_box).await?;
    let stake_box = ergo_box_to_eip12(client, &stake_ergo_box).await?;

    // Pay the executor tip (OUTPUTS[3], script-free) to the proxy's own R5 recipient so
    // the key-holder collects it when they self-execute the payout.
    let executor_tx = build_paideia_executor_tx(
        &state_box,
        &state_snapshot,
        &stake_box,
        &stake,
        &proxy_box,
        &recipient_tree_from_proxy(&proxy_box)?,
        current_height,
    )
    .str_err()?;
    let executor_json = eip12_tx_to_node_json(&executor_tx)?;

    Ok((executor_json, refund_json, proxy_box.box_id.clone()))
}

/// Read the payout recipient's ErgoTree (hex) from a proxy box's R5 register.
fn recipient_tree_from_proxy(proxy_box: &ergo_tx::Eip12InputBox) -> Result<String, String> {
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

/// Resolve the unstake proxy box (OUTPUT[0]) created by a confirmed step-1 tx. Returns
/// the proxy box id, or an error while the tx is still unconfirmed / not the proxy shape.
/// The proxy must be confirmed (in the UTXO set) before its spend paths can be checked.
#[tauri::command]
pub async fn paideia_proxy_box_id(
    state: State<'_, AppState>,
    tx_id: String,
) -> Result<String, String> {
    let client = state.require_node_client().await?;
    let tx = client
        .get_transaction_by_id(&tx_id)
        .await
        .map_err(|e| format!("Step-1 tx {} not confirmed yet: {}", tx_id, e))?;
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

/// Dry-run BOTH permissionless spend paths of a confirmed Paideia proxy box through the
/// node's `/transactions/check` without broadcasting. This is the execution-time safety
/// gate: after step 1 confirms, the UI shows whether the reward payout and the refund
/// both validate against the *exact* proxy box step 1 created, before the user commits to
/// either. Neither tx needs a signature.
#[tauri::command]
pub async fn check_paideia_proxy(
    state: State<'_, AppState>,
    proxy_box_id: String,
) -> Result<PaideiaProxyCheck, String> {
    let client = state.require_node_client().await?;
    let (executor_json, refund_json, box_id) =
        assemble_paideia_proxy_txs(&client, &proxy_box_id).await?;

    let executor = match client.check_transaction(&executor_json).await {
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
#[tauri::command]
pub async fn submit_paideia_proxy_tx(
    state: State<'_, AppState>,
    proxy_box_id: String,
    which: String,
) -> Result<String, String> {
    let client = state.require_node_client().await?;
    let (executor_json, refund_json, _box_id) =
        assemble_paideia_proxy_txs(&client, &proxy_box_id).await?;

    let tx_json = match which.as_str() {
        "executor" => executor_json,
        "refund" => refund_json,
        other => return Err(format!("Unknown proxy path '{}'", other)),
    };

    // Never broadcast a tx the node would reject.
    client
        .check_transaction(&tx_json)
        .await
        .map_err(|e| format!("Pre-broadcast check failed, not submitting: {}", e))?;

    client.submit_transaction(&tx_json).await.str_err()
}
