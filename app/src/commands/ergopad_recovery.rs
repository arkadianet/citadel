use citadel_api::AppState;
use ergopad_recovery::{
    build_recovery_tx_eip12, discover_recoverable_stakes, fetch_stake_box_by_key,
    fetch_stake_state, parse_stake_box, RecoverableStake, RecoveryScan,
};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use tauri::State;

use super::StrErr;

/// Scan the v1 ergopad staking contract for StakeBoxes whose R5 matches any of
/// the provided candidate token IDs (typically the user's wallet's unique-qty-1 tokens).
#[tauri::command]
pub async fn scan_ergopad_recoverable_stakes(
    state: State<'_, AppState>,
    candidate_token_ids: Vec<String>,
) -> Result<RecoveryScan, String> {
    let client = state.require_node_client().await?;
    discover_recoverable_stakes(&client, &candidate_token_ids)
        .await
        .str_err()
}

/// Build the 3-input recovery tx for a single stake key.
/// `user_utxos` must include the box holding the stake key NFT.
#[tauri::command]
pub async fn build_ergopad_recovery_tx(
    state: State<'_, AppState>,
    stake_key_id: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;

    let (state_ergo_box, state_snapshot) = fetch_stake_state(&client).await.str_err()?;
    let stake_ergo_box = fetch_stake_box_by_key(&client, &stake_key_id)
        .await
        .str_err()?;
    let stake = parse_stake_box(&stake_ergo_box).str_err()?;

    let state_box = ergo_box_to_eip12(&client, &state_ergo_box).await?;
    let stake_box = ergo_box_to_eip12(&client, &stake_ergo_box).await?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

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

/// Expose the already-parsed stake for a given key (used by the UI to render confirm dialogs).
#[tauri::command]
pub async fn preview_ergopad_recovery(
    state: State<'_, AppState>,
    stake_key_id: String,
) -> Result<RecoverableStake, String> {
    let client = state.require_node_client().await?;
    let stake_ergo_box = fetch_stake_box_by_key(&client, &stake_key_id)
        .await
        .str_err()?;
    parse_stake_box(&stake_ergo_box).str_err()
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
    Ok(ergo_tx::Eip12InputBox::from_ergo_box(ergo_box, tx_id, index))
}
