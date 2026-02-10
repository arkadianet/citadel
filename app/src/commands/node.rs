use citadel_api::dto::{HealthResponse, NodeConfigRequest, NodeStatusResponse};
use citadel_api::AppState;
use citadel_core::NodeConfig;
use tauri::State;

/// Health check command
#[tauri::command]
pub async fn health_check() -> Result<HealthResponse, String> {
    Ok(HealthResponse::default())
}

/// Get node connection status
#[tauri::command]
pub async fn get_node_status(state: State<'_, AppState>) -> Result<NodeStatusResponse, String> {
    let config = state.config().await;

    // Try to get node client and capabilities
    let client = state.node_client().await;

    match client {
        Some(client) => {
            // Refresh capabilities
            client.refresh_capabilities().await;

            let caps = client.capabilities().await;
            let node_name = client.node_name().await;

            match caps {
                Some(caps) => Ok(NodeStatusResponse {
                    connected: caps.is_online,
                    url: config.node.url,
                    node_name,
                    network: config.network.as_str().to_string(),
                    chain_height: caps.chain_height,
                    indexed_height: caps.indexed_height,
                    capability_tier: caps.capability_tier.as_str().to_string(),
                    index_lag: caps.index_lag(),
                }),
                None => Ok(NodeStatusResponse {
                    connected: true,
                    url: config.node.url,
                    node_name,
                    network: config.network.as_str().to_string(),
                    chain_height: 0,
                    indexed_height: None,
                    capability_tier: "Basic".to_string(),
                    index_lag: None,
                }),
            }
        }
        None => Ok(NodeStatusResponse {
            connected: false,
            url: config.node.url,
            node_name: None,
            network: config.network.as_str().to_string(),
            chain_height: 0,
            indexed_height: None,
            capability_tier: "Basic".to_string(),
            index_lag: None,
        }),
    }
}

/// Configure node connection
#[tauri::command]
pub async fn configure_node(
    state: State<'_, AppState>,
    request: NodeConfigRequest,
) -> Result<NodeStatusResponse, String> {
    // Update config
    let node_config = NodeConfig {
        url: request.url,
        api_key: request.api_key,
    };
    state.set_node_config(node_config).await;

    // Refresh client and return status
    let _ = state.refresh_node_client().await;

    get_node_status(state).await
}

// =============================================================================
// Node Discovery
// =============================================================================

/// Hardcoded known-good public nodes
const PUBLIC_NODES: &[&str] = &[
    "https://node.sigmaspace.io",
    "https://ergo-node.eutxo.de",
    "https://node.ergo.watch",
];

/// Discover and probe available nodes.
/// Starts with hardcoded public nodes, adds peers from connected node if available.
#[tauri::command]
pub async fn discover_nodes(
    state: State<'_, AppState>,
) -> Result<Vec<ergo_node_client::NodeProbeResult>, String> {
    use std::collections::HashSet;

    // Start with hardcoded nodes
    let mut urls: Vec<String> = PUBLIC_NODES.iter().map(|s| s.to_string()).collect();
    let mut seen: HashSet<String> = urls.iter().cloned().collect();

    // If connected, fetch peers and derive API URLs
    if let Some(client) = state.node_client().await {
        if let Ok(peers) = client.get_connected_peers().await {
            for peer in peers {
                // Peer address is like "/1.2.3.4:9030" — extract IP, replace port with 9053
                let addr = peer.address.trim_start_matches('/');
                if let Some(ip) = addr.split(':').next() {
                    let candidate = format!("http://{}:9053", ip);
                    if seen.insert(candidate.clone()) {
                        urls.push(candidate);
                    }
                }
            }
        }
    }

    // Probe all in parallel
    let futures: Vec<_> = urls
        .into_iter()
        .map(|url| async move { ergo_node_client::probe_node(&url).await })
        .collect();

    let results = futures::future::join_all(futures).await;

    // Filter out failures, sort by tier (Full first) then latency
    let mut nodes: Vec<ergo_node_client::NodeProbeResult> = results.into_iter().flatten().collect();

    nodes.sort_by(|a, b| {
        let tier_ord = |t: &str| match t {
            "Full" => 0,
            "IndexLagging" => 1,
            _ => 2,
        };
        tier_ord(&a.capability_tier)
            .cmp(&tier_ord(&b.capability_tier))
            .then(a.latency_ms.cmp(&b.latency_ms))
    });

    Ok(nodes)
}

/// Probe a single node URL — used to show capability badge for a custom URL.
#[tauri::command]
pub async fn probe_single_node(
    url: String,
) -> Result<Option<ergo_node_client::NodeProbeResult>, String> {
    Ok(ergo_node_client::probe_node(&url).await)
}
