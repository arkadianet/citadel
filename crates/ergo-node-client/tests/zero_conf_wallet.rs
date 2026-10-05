//! Wallet spends must follow the terminal mempool outputs or fail with an error.

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use citadel_core::NodeConfig;
use ergo_node_client::{address_to_ergo_tree, NodeClient};
use serde_json::{json, Value};

const ADDRESS: &str = "9eoLQ6FFKJPqZXeBFvd3CKu7DRfXavKo7n9PFkVypSmXgD6ActU";

struct MockData {
    confirmed: Vec<Value>,
    transactions: Vec<Value>,
    mempool_error: Option<StatusCode>,
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
    let offset: usize = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .find_map(|part| part.strip_prefix("offset="))
        .unwrap_or("0")
        .parse()
        .unwrap();
    let data = data.lock().unwrap();
    match path.as_str() {
        "/info" => Json(json!({ "fullHeight": 100, "extraIndex": true })).into_response(),
        "/blockchain/indexedHeight" => {
            Json(json!({ "indexedHeight": 100, "fullHeight": 100 })).into_response()
        }
        "/blockchain/box/unspent/byAddress" => Json(&data.confirmed).into_response(),
        "/transactions/unconfirmed/byErgoTree" => {
            if let Some(status) = data.mempool_error {
                return (status, Json(json!({ "error": "mempool unavailable" }))).into_response();
            }
            Json(
                data.transactions
                    .iter()
                    .skip(offset)
                    .take(100)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mock_node(confirmed: Vec<Value>, transactions: Vec<Value>) -> MockNode {
    let data = Arc::new(Mutex::new(MockData {
        confirmed,
        transactions,
        mempool_error: None,
    }));
    let app = Router::new()
        .route("/transactions/unconfirmed/byErgoTree", post(mock_response))
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

fn wallet_box(id: &str) -> Value {
    json!({
        "boxId": id,
        "transactionId": "11".repeat(32),
        "index": 0,
        "value": 10_000_000,
        "ergoTree": address_to_ergo_tree(ADDRESS).unwrap(),
        "creationHeight": 100,
        "assets": [{ "tokenId": "22".repeat(32), "amount": 1 }],
        "additionalRegisters": {}
    })
}

fn pending(id: &str, input: &str, output: &str) -> Value {
    let mut output = wallet_box(output);
    output.as_object_mut().unwrap().remove("transactionId");
    output.as_object_mut().unwrap().remove("index");
    json!({ "id": id, "inputs": [{ "boxId": input }], "outputs": [output] })
}

#[tokio::test]
async fn mempool_errors_never_restore_confirmed_wallet_inputs() {
    let node = mock_node(vec![wallet_box("confirmed")], vec![]).await;
    for status in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        node.data.lock().unwrap().mempool_error = Some(status);
        let error = node
            .client
            .get_effective_utxos(ADDRESS)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Cannot check pending wallet transactions"));
    }
    node.data.lock().unwrap().mempool_error = None;
    let confirmed = node.client.get_effective_utxos(ADDRESS).await.unwrap();
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].box_id, "confirmed");
}

#[tokio::test]
async fn malformed_mempool_transactions_never_restore_confirmed_wallet_inputs() {
    for malformed in [
        json!({ "id": "33".repeat(32), "outputs": [] }),
        json!({ "id": "33".repeat(32), "inputs": [{}], "outputs": [] }),
        json!({ "inputs": [{ "boxId": "confirmed" }], "outputs": [] }),
        json!({ "id": "33".repeat(32), "inputs": [{ "boxId": "confirmed" }] }),
    ] {
        let node = mock_node(vec![wallet_box("confirmed")], vec![malformed]).await;
        let error = node
            .client
            .get_effective_utxos(ADDRESS)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("malformed mempool transaction"));
    }
}

#[tokio::test]
async fn chained_wallet_outputs_work_across_pages_and_transaction_order() {
    let first = pending(&"33".repeat(32), "confirmed", "intermediate");
    let second = pending(&"44".repeat(32), "intermediate", "terminal");
    let mut transactions = (0..100)
        .map(|index| {
            json!({ "id": format!("{index:064x}"), "inputs": [{ "boxId": "other" }], "outputs": [] })
        })
        .collect::<Vec<_>>();
    transactions.extend([second, first.clone(), first]);
    let node = mock_node(
        vec![wallet_box("confirmed"), wallet_box("unrelated")],
        transactions,
    )
    .await;
    let effective = node.client.get_effective_utxos(ADDRESS).await.unwrap();
    assert_eq!(effective.len(), 2);
    assert_eq!(effective[0].box_id, "unrelated");
    assert_eq!(effective[1].box_id, "terminal");
    assert_eq!(effective[1].transaction_id, "44".repeat(32));
    assert_eq!(effective[1].index, 0);
    assert_eq!(effective[1].assets[0].amount, "1");
}
