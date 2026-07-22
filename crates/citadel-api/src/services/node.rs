//! Node connection status, configuration, and discovery.

use citadel_core::NodeConfig;

use crate::dto::{HealthResponse, NodeConfigRequest, NodeStatusResponse};

use super::error::ServiceResult;
use crate::AppState;

/// Hardcoded known-good public nodes
const PUBLIC_NODES: &[&str] = &[
    "https://node.sigmaspace.io",
    "https://ergo-node.eutxo.de",
    "https://node.ergo.watch",
];

pub fn health_check() -> HealthResponse {
    HealthResponse::default()
}

pub async fn get_node_status(state: &AppState) -> ServiceResult<NodeStatusResponse> {
    let config = state.config().await;
    let client = state.node_client().await;

    match client {
        Some(client) => {
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

pub async fn configure_node(
    state: &AppState,
    request: NodeConfigRequest,
) -> ServiceResult<NodeStatusResponse> {
    let node_config = NodeConfig {
        url: request.url,
        api_key: request.api_key,
    };
    state.set_node_config(node_config).await;
    let _ = state.refresh_node_client().await;
    get_node_status(state).await
}

pub async fn discover_nodes(
    state: &AppState,
) -> ServiceResult<Vec<ergo_node_client::NodeProbeResult>> {
    use std::collections::HashSet;

    let mut urls: Vec<String> = PUBLIC_NODES.iter().map(|s| s.to_string()).collect();
    let mut seen: HashSet<String> = urls.iter().cloned().collect();

    if let Some(client) = state.node_client().await {
        if let Ok(peers) = client.get_connected_peers().await {
            for peer in peers {
                if let Some(rest) = peer.rest_api_url {
                    let rest = rest.trim_end_matches('/').to_string();
                    if seen.insert(rest.clone()) {
                        urls.push(rest);
                    }
                    continue;
                }
                let addr = peer.address.trim_start_matches('/');
                if let Some(ip) = addr.split(':').next() {
                    for port in [9053u16, 9063] {
                        let candidate = format!("http://{}:{}", ip, port);
                        if seen.insert(candidate.clone()) {
                            urls.push(candidate);
                        }
                    }
                }
            }
        }
    }

    let futures: Vec<_> = urls
        .into_iter()
        .map(|url| async move { ergo_node_client::probe_node(&url).await })
        .collect();

    let results = futures::future::join_all(futures).await;

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

pub async fn probe_single_node(
    url: &str,
) -> ServiceResult<Option<ergo_node_client::NodeProbeResult>> {
    Ok(ergo_node_client::probe_node(url).await)
}
