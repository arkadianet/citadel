use citadel_api::services::stake_recovery as stake_svc;
use citadel_api::AppState;
use stake_recovery::{RecoverableStake, RecoveryScan};
use tauri::State;

/// Scan every registered v1 staking protocol (Ergopad, EGIO, …) for StakeBoxes whose
/// R5 matches any of the provided candidate token IDs (typically the wallet's
/// unique-qty-1 tokens). Auto-detects which protocol each recovered stake belongs to.
#[tauri::command]
pub async fn scan_recoverable_stakes(
    state: State<'_, AppState>,
    candidate_token_ids: Vec<String>,
) -> Result<RecoveryScan, String> {
    stake_svc::scan_recoverable_stakes(&state, &candidate_token_ids).await
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
    let parsed = super::parse_eip12_utxos(user_utxos)?;
    let tx = stake_svc::build_recovery_tx(&state, &stake_key_id, parsed, current_height).await?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize transaction: {}", e))
}

/// Expose the parsed stake for a given key (used by the UI to render confirm
/// dialogs). Auto-detects the owning protocol.
#[tauri::command]
pub async fn preview_recovery(
    state: State<'_, AppState>,
    stake_key_id: String,
) -> Result<RecoverableStake, String> {
    stake_svc::preview_recovery(&state, &stake_key_id).await
}

/// Resolve the unstake proxy box (OUTPUT[0]) created by a confirmed step-1 tx.
#[tauri::command]
pub async fn paideia_proxy_box_id(
    state: State<'_, AppState>,
    tx_id: String,
) -> Result<String, String> {
    stake_svc::paideia_proxy_box_id(&state, &tx_id).await
}

/// Dry-run both permissionless spend paths of a confirmed Paideia proxy box without
/// broadcasting.
#[tauri::command]
pub async fn check_paideia_proxy(
    state: State<'_, AppState>,
    proxy_box_id: String,
) -> Result<stake_svc::PaideiaProxyCheck, String> {
    stake_svc::check_paideia_proxy(&state, &proxy_box_id).await
}

/// Broadcast one permissionless Paideia proxy spend path. `which` is `"executor"` or
/// `"refund"`.
#[tauri::command]
pub async fn submit_paideia_proxy_tx(
    state: State<'_, AppState>,
    proxy_box_id: String,
    which: String,
) -> Result<String, String> {
    stake_svc::submit_paideia_proxy_tx(&state, &proxy_box_id, &which).await
}
