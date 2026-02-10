use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::AppState;
use citadel_core::BoxId;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::NodeClient;
use ergo_tx::Eip12UnsignedTx;
use ergopay_core::{reduce_transaction, reduce_transaction_fallback};
use ergopay_server::RequestStatus;
use tauri::State;

/// Start ErgoPay signing flow for a mint transaction
///
/// This function performs transaction reduction to convert the EIP-12 transaction
/// into sigma-serialized ReducedTransaction bytes, which is the format required
/// by mobile wallets via the ErgoPay protocol.
#[tauri::command]
pub async fn start_mint_sign(
    state: State<'_, AppState>,
    request: MintSignRequest,
) -> Result<MintSignResponse, String> {
    // Get node client for fetching boxes and state context
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    // Parse the unsigned transaction from JSON
    let eip12_tx: Eip12UnsignedTx = serde_json::from_value(request.unsigned_tx.clone())
        .map_err(|e| format!("Failed to parse unsigned tx: {}", e))?;

    // Fetch all input boxes as ErgoBox instances
    let input_boxes = fetch_boxes_by_ids(
        &client,
        &eip12_tx
            .inputs
            .iter()
            .map(|i| &i.box_id)
            .collect::<Vec<_>>(),
    )
    .await?;

    // Fetch all data input boxes as ErgoBox instances
    let data_input_boxes = fetch_boxes_by_ids(
        &client,
        &eip12_tx
            .data_inputs
            .iter()
            .map(|d| &d.box_id)
            .collect::<Vec<_>>(),
    )
    .await?;

    // Reduce the transaction to sigma-serialized bytes (EIP-19 format).
    // Try sigma-rust's reduce_tx first; fall back to manual byte construction
    // for transactions with ErgoTrees that sigma-rust cannot parse (e.g. Phoenix bank boxes).
    let reduced_bytes =
        match reduce_transaction(&eip12_tx, input_boxes, data_input_boxes, &client).await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Standard reduction failed ({}), using fallback", e);
                reduce_transaction_fallback(&eip12_tx)
                    .map_err(|e| format!("Fallback reduction also failed: {}", e))?
            }
        };

    // Get ErgoPay server from app state
    let server = state.ergopay_server().await.map_err(|e| e.to_string())?;

    // Create signing request with reduced bytes and unsigned tx
    let (request_id, ergopay_url) = server
        .create_tx_request(reduced_bytes, request.unsigned_tx.clone(), request.message)
        .await;

    // Get Nautilus URL
    let nautilus_url = server.get_nautilus_url(&request_id);

    Ok(MintSignResponse {
        request_id,
        ergopay_url,
        nautilus_url,
    })
}

/// Helper to fetch multiple boxes by their IDs
pub(super) async fn fetch_boxes_by_ids(
    client: &NodeClient,
    box_ids: &[&String],
) -> Result<Vec<ErgoBox>, String> {
    let mut boxes = Vec::with_capacity(box_ids.len());
    for box_id in box_ids {
        let ergo_box =
            ergo_node_client::queries::get_box_by_id(client.inner(), &BoxId::new(box_id.as_str()))
                .await
                .map_err(|e| format!("Failed to fetch box {}: {}", box_id, e))?;
        boxes.push(ergo_box);
    }
    Ok(boxes)
}

/// Get status of a mint transaction signing request
#[tauri::command]
pub async fn get_mint_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    let server = state.ergopay_server().await.map_err(|e| e.to_string())?;

    match server.get_request_status(&request_id).await {
        Some(RequestStatus::Pending) => Ok(MintTxStatusResponse {
            status: "pending".to_string(),
            tx_id: None,
            error: None,
        }),
        Some(RequestStatus::TxSubmitted { tx_id }) => Ok(MintTxStatusResponse {
            status: "submitted".to_string(),
            tx_id: Some(tx_id),
            error: None,
        }),
        Some(RequestStatus::AddressReceived(_)) => Ok(MintTxStatusResponse {
            // AddressReceived is for connect requests, not mint tx
            status: "pending".to_string(),
            tx_id: None,
            error: None,
        }),
        Some(RequestStatus::Expired) => Ok(MintTxStatusResponse {
            status: "expired".to_string(),
            tx_id: None,
            error: Some("Request expired".to_string()),
        }),
        Some(RequestStatus::Failed(msg)) => Ok(MintTxStatusResponse {
            status: "failed".to_string(),
            tx_id: None,
            error: Some(msg),
        }),
        None => Ok(MintTxStatusResponse {
            status: "unknown".to_string(),
            tx_id: None,
            error: Some("Request not found".to_string()),
        }),
    }
}

/// Open Nautilus page in the user's default browser.
///
/// If the default browser is Chrome/Chromium, uses `--app=URL` mode for a
/// standalone window where `window.close()` works. Otherwise opens normally
/// in the default browser so the Nautilus extension is available.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn open_nautilus(app: tauri::AppHandle, nautilusUrl: String) -> Result<(), String> {
    // On Linux, check the default browser via xdg-settings
    if let Ok(output) = std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        let default = String::from_utf8_lossy(&output.stdout).to_lowercase();
        // If default browser is Chrome-based, use --app mode for standalone window
        if default.contains("chrome") || default.contains("chromium") {
            let candidates = if default.contains("chromium") {
                &["chromium-browser", "chromium"][..]
            } else {
                &["google-chrome-stable", "google-chrome"][..]
            };

            for name in candidates {
                if let Ok(child) = std::process::Command::new(name)
                    .args([&format!("--app={}", nautilusUrl), "--window-size=500,650"])
                    .spawn()
                {
                    tracing::info!(
                        "Opened Nautilus page with {} --app mode (pid {:?})",
                        name,
                        child.id()
                    );
                    return Ok(());
                }
            }
        }
    }

    // Default: open in the user's default browser via Tauri opener plugin
    tracing::info!("Opening Nautilus page in default browser");
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&nautilusUrl, None::<&str>)
        .map_err(|e| format!("Failed to open browser: {}", e))
}
