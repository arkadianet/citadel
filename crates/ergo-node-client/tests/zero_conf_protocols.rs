//! Shared protocol discovery follows pending replacements without losing pages.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use citadel_core::{NodeConfig, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::{address_to_ergo_tree, NodeClient};
use serde_json::{json, Value};

const ADDRESS: &str = "9eoLQ6FFKJPqZXeBFvd3CKu7DRfXavKo7n9PFkVypSmXgD6ActU";
const TOKEN: &str = "2222222222222222222222222222222222222222222222222222222222222222";

struct MockData {
    confirmed: Vec<Value>,
    transactions: Vec<Value>,
    mempool_error: Option<StatusCode>,
    by_id: Option<Value>,
}

struct MockNode {
    client: NodeClient,
    data: Arc<Mutex<MockData>>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for MockNode {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn mock_response(State(data): State<Arc<Mutex<MockData>>>, request: Request) -> Response {
    let path = request.uri().path().to_string();
    let params = request.uri().query().unwrap_or("");
    let get_param = |key: &str, fallback: usize| {
        params
            .split('&')
            .find_map(|part| part.strip_prefix(key))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(fallback)
    };
    let offset = get_param("offset=", 0);
    let limit = get_param("limit=", 500);
    let data = data.lock().unwrap();
    match path.as_str() {
        "/info" => Json(json!({ "fullHeight": 100, "extraIndex": true })).into_response(),
        "/blockchain/indexedHeight" => {
            Json(json!({ "indexedHeight": 100, "fullHeight": 100 })).into_response()
        }
        "/transactions/unconfirmed" | "/transactions/unconfirmed/byErgoTree" => {
            if let Some(status) = data.mempool_error {
                return (status, Json(json!({ "error": "mempool unavailable" }))).into_response();
            }
            Json(
                data.transactions
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        path if path.starts_with("/blockchain/box/unspent/") => Json(
            data.confirmed
                .iter()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_response(),
        path if path.starts_with("/utxo/withPool/byId/") => match &data.by_id {
            Some(box_value) => Json(box_value).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mock_node(confirmed: Vec<Value>, transactions: Vec<Value>) -> MockNode {
    let data = Arc::new(Mutex::new(MockData {
        confirmed,
        transactions,
        mempool_error: None,
        by_id: None,
    }));
    let app = Router::new()
        .fallback(mock_response)
        .with_state(data.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = NodeClient::new(NodeConfig {
        url,
        api_key: String::new(),
    })
    .await
    .unwrap();
    MockNode {
        client,
        data,
        server,
    }
}

fn box_value(tx_id: &str, index: u16, value: u64) -> ErgoBox {
    serde_json::from_value(json!({
        "transactionId": tx_id,
        "index": index,
        "value": value,
        "ergoTree": address_to_ergo_tree(ADDRESS).unwrap(),
        "creationHeight": 100,
        "assets": [{ "tokenId": TOKEN, "amount": 1 }],
        "additionalRegisters": {}
    }))
    .unwrap()
}

fn transaction(tx_id: &str, inputs: &[String], output: &ErgoBox) -> Value {
    let mut output = serde_json::to_value(output).unwrap();
    output.as_object_mut().unwrap().remove("transactionId");
    output.as_object_mut().unwrap().remove("index");
    json!({
        "id": tx_id,
        "inputs": inputs.iter().map(|id| json!({ "boxId": id })).collect::<Vec<_>>(),
        "outputs": [output]
    })
}

#[tokio::test]
async fn singleton_address_and_tree_discovery_choose_terminal_unconfirmed_state() {
    let original = box_value(&"11".repeat(32), 0, 10_000_000);
    let intermediate = box_value(&"33".repeat(32), 0, 9_000_000);
    let terminal = box_value(&"44".repeat(32), 0, 8_000_000);
    let first = transaction(
        &"33".repeat(32),
        &[original.box_id().to_string()],
        &intermediate,
    );
    let second = transaction(
        &"44".repeat(32),
        &[intermediate.box_id().to_string()],
        &terminal,
    );
    let node = mock_node(
        vec![serde_json::to_value(&original).unwrap()],
        vec![second, first.clone(), first],
    )
    .await;
    let caps = node.client.capabilities().await.unwrap();
    let by_token = node
        .client
        .get_box_by_token_id(&caps, &TokenId::new(TOKEN))
        .await
        .unwrap();
    let by_address = node
        .client
        .unspent_boxes_by_address(ADDRESS, 0, 500)
        .await
        .unwrap();
    let by_tree = node
        .client
        .unspent_boxes_by_ergo_tree(&address_to_ergo_tree(ADDRESS).unwrap(), 0, 500)
        .await
        .unwrap();
    for selected in [by_token, by_address[0].clone(), by_tree[0].clone()] {
        assert_eq!(selected.box_id(), terminal.box_id());
        assert_eq!(selected.transaction_id.to_string(), "44".repeat(32));
        assert_eq!(selected.index, 0);
    }
    assert_eq!(by_address.len(), 1);
    assert_eq!(by_tree.len(), 1);
    let (confirmed_erg, _) = node.client.get_address_balances(ADDRESS).await.unwrap();
    assert_eq!(
        confirmed_erg, 10_000_000,
        "confirmed baseline stays confirmed"
    );
    let effective = node.client.get_effective_utxos(ADDRESS).await.unwrap();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].value, "8000000");
}

#[tokio::test]
async fn effective_pagination_fills_pages_after_pending_spends_and_never_duplicates() {
    let confirmed = (0..502)
        .map(|index| box_value(&"11".repeat(32), index, 10_000_000))
        .collect::<Vec<_>>();
    let replacement = box_value(&"33".repeat(32), 0, 20_000_000);
    let spent = [
        confirmed[0].box_id().to_string(),
        confirmed[500].box_id().to_string(),
    ];
    let pending = transaction(&"33".repeat(32), &spent, &replacement);
    let node = mock_node(
        confirmed
            .iter()
            .map(|b| serde_json::to_value(b).unwrap())
            .collect(),
        vec![pending],
    )
    .await;
    let first = node
        .client
        .unspent_boxes_by_address(ADDRESS, 0, 500)
        .await
        .unwrap();
    let second = node
        .client
        .unspent_boxes_by_address(ADDRESS, 500, 500)
        .await
        .unwrap();
    assert_eq!(
        first.len(),
        500,
        "pending spend must not cause premature last-page detection"
    );
    assert_eq!(second.len(), 1);
    let ids = first
        .iter()
        .chain(&second)
        .map(|b| b.box_id().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 501);
    assert!(ids.contains(&replacement.box_id().to_string()));
    assert!(spent.iter().all(|id| !ids.contains(id)));
    assert!(node
        .client
        .unspent_boxes_by_address(ADDRESS, 1000, 500)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn protocol_discovery_and_native_creation_info_are_safe_for_pending_boxes() {
    let original = box_value(&"11".repeat(32), 0, 10_000_000);
    let node = mock_node(vec![serde_json::to_value(&original).unwrap()], vec![]).await;
    let caps = node.client.capabilities().await.unwrap();
    for status in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        node.data.lock().unwrap().mempool_error = Some(status);
        assert!(node
            .client
            .get_box_by_token_id(&caps, &TokenId::new(TOKEN))
            .await
            .is_err());
        assert!(node
            .client
            .unspent_boxes_by_address(ADDRESS, 0, 500)
            .await
            .is_err());
        assert!(node
            .client
            .unspent_boxes_by_ergo_tree(&address_to_ergo_tree(ADDRESS).unwrap(), 0, 500)
            .await
            .is_err());
    }
    node.data.lock().unwrap().by_id = Some(serde_json::to_value(&original).unwrap());
    let info = node
        .client
        .get_box_creation_info(&original.box_id().to_string())
        .await
        .unwrap();
    assert_eq!(info, (original.transaction_id.to_string(), original.index));
}
