use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::services::signing as sign_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn start_mint_sign(
    state: State<'_, AppState>,
    request: MintSignRequest,
) -> Result<MintSignResponse, String> {
    sign_svc::start_mint_sign(&state, request).await
}

#[tauri::command]
pub async fn get_mint_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    sign_svc::get_mint_tx_status(&state, &request_id).await
}

/// Open Nautilus page in the user's default browser.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn open_nautilus(app: tauri::AppHandle, nautilusUrl: String) -> Result<(), String> {
    if let Ok(output) = std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        let default = String::from_utf8_lossy(&output.stdout).to_lowercase();
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

    tracing::info!("Opening Nautilus page in default browser");
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&nautilusUrl, None::<&str>)
        .map_err(|e| format!("Failed to open browser: {}", e))
}
