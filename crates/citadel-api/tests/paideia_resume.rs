//! Recovery discovery and resume against a local node, including unconfirmed proxies.
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use citadel_api::{services::stake_recovery as recovery, AppState};
use ergo_lib::ergotree_ir::chain::{
    address::{Address, AddressEncoder, NetworkPrefix},
    ergo_box::ErgoBox,
};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use serde_json::{json, Value};
use stake_recovery::PAIDEIA_PROXY_ERGO_TREE;

const OWNER: &str = "0008cd039d3e8d055631b2926120f2f1cb69709c1da1f52077358a319a747f02d83253fb";
const OTHER_OWNER: &str =
    "0008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6";

fn proxy(seed: &str, tree: &str, height: u32) -> ErgoBox {
    serde_json::from_value(json!({
        "transactionId": seed.repeat(32), "index": 0,
        "value": 113_000_000, "ergoTree": PAIDEIA_PROXY_ERGO_TREE,
        "creationHeight": height,
        "assets": [{"tokenId": seed.repeat(32), "amount":1}],
        "additionalRegisters": {"R4":"1101f0a48cab06", "R5":format!("0e24{tree}")}
    }))
    .unwrap()
}

fn pending(b: &ErgoBox) -> Value {
    let mut output = serde_json::to_value(b).unwrap();
    // Real unconfirmed transaction outputs need not include creation metadata.
    output.as_object_mut().unwrap().remove("transactionId");
    output.as_object_mut().unwrap().remove("index");
    json!({"id": b.transaction_id.to_string(), "inputs": [], "outputs": [output]})
}

#[derive(Default)]
struct NodeData {
    confirmed: Vec<Value>,
    pending: Vec<Value>,
    live_proxy: Option<Value>,
    requests: Vec<String>,
    mempool_error: bool,
}

async fn respond(State(data): State<Arc<Mutex<NodeData>>>, request: Request) -> Response {
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
    let limit: usize = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .find_map(|part| part.strip_prefix("limit="))
        .unwrap_or("100")
        .parse()
        .unwrap();
    let body = axum::body::to_bytes(request.into_body(), 65_536)
        .await
        .unwrap();
    let mut data = data.lock().unwrap();
    data.requests.push(path.clone());
    match path.as_str() {
        "/info" => Json(json!({"fullHeight":1882943,"extraIndex":true})).into_response(),
        "/blockchain/indexedHeight" => {
            Json(json!({"indexedHeight":1882943,"fullHeight":1882943})).into_response()
        }
        "/transactions/unconfirmed" => {
            if data.mempool_error {
                return (StatusCode::SERVICE_UNAVAILABLE, "mempool unavailable").into_response();
            }
            Json(
                data.pending
                    .iter()
                    .skip(offset)
                    .take(100)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        "/blockchain/box/unspent/byAddress" => {
            let address: String = serde_json::from_slice(&body).unwrap();
            if address == stake_recovery::PAIDEIA_PROXY_ADDRESS {
                Json(
                    data.confirmed
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .cloned()
                        .collect::<Vec<_>>(),
                )
                .into_response()
            } else {
                Json(json!([])).into_response()
            }
        }
        "/transactions/check" => {
            let tx: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                tx["inputs"].as_array().unwrap().len(),
                1,
                "only the refund should be checked without a live stake"
            );
            assert_eq!(tx["outputs"][0]["ergoTree"], OWNER);
            Json(json!("accepted-refund")).into_response()
        }
        _ if path.starts_with("/utxo/withPool/byId/") => match &data.live_proxy {
            Some(b) => Json(b.clone()).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        _ if path.starts_with("/transactions/unconfirmed/byTransactionId/") => {
            Json(data.pending.first().cloned().unwrap_or(json!({}))).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "confirmed-only lookup unavailable").into_response(),
    }
}

struct TestNode {
    state: AppState,
    data: Arc<Mutex<NodeData>>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for TestNode {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn node(data: NodeData) -> TestNode {
    let data = Arc::new(Mutex::new(data));
    let app = Router::new().fallback(respond).with_state(data.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = citadel_core::AppConfig::default();
    config.node = citadel_core::NodeConfig {
        url: format!("http://{}", listener.local_addr().unwrap()),
        api_key: String::new(),
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let state = AppState::with_config(config);
    let tree = ErgoTree::sigma_parse_bytes(&hex::decode(OWNER).unwrap()).unwrap();
    let address = Address::recreate_from_ergo_tree(&tree).unwrap();
    state
        .set_wallet(AddressEncoder::new(NetworkPrefix::Mainnet).address_to_str(&address))
        .await
        .unwrap();
    TestNode {
        state,
        data,
        server,
    }
}

#[tokio::test]
async fn discovers_old_and_unconfirmed_wallet_proxies_excluding_spent_and_foreign() {
    let old = proxy("01", OWNER, 1_000_000);
    let spent = proxy("02", OWNER, 1_000_001);
    let foreign = proxy("03", OTHER_OWNER, 1_000_002);
    let intermediate = proxy("04", OWNER, 1_882_943);
    let current = proxy("05", OWNER, 1_882_943);
    let child = json!({"id":"06".repeat(32), "inputs":[{"boxId":spent.box_id().to_string()}, {"boxId":intermediate.box_id().to_string()}], "outputs":[]});
    let test = node(NodeData {
        confirmed: vec![
            serde_json::to_value(&old).unwrap(),
            serde_json::to_value(&spent).unwrap(),
            serde_json::to_value(&foreign).unwrap(),
            serde_json::to_value(&current).unwrap(),
        ],
        // Children before parents and duplicates must not revive spent proxies.
        pending: vec![
            child,
            pending(&intermediate),
            pending(&current),
            pending(&current),
        ],
        ..NodeData::default()
    })
    .await;
    let scan = recovery::scan_pending_paideia_recoveries(&test.state)
        .await
        .unwrap();
    assert!(!scan.hit_page_limit);
    assert_eq!(scan.recoveries.len(), 2);
    assert_eq!(
        scan.recoveries[0].creation_tx_id,
        current.transaction_id.to_string()
    );
    assert!(!scan.recoveries[0].confirmed);
    assert_eq!(scan.recoveries[0].reward_amount_display, "85049.5800");
    assert_eq!(scan.recoveries[1].proxy_box_id, old.box_id().to_string());
    assert!(scan.recoveries[1].confirmed);
    assert!(scan
        .recoveries
        .iter()
        .all(|r| !r.recipient_address.is_empty()));
}

#[tokio::test]
async fn resumes_unconfirmed_proxy_and_keeps_refund_available_when_executor_cannot_build() {
    let b = proxy("07", OWNER, 1_882_943);
    let test = node(NodeData {
        live_proxy: Some(serde_json::to_value(&b).unwrap()),
        pending: vec![pending(&b)],
        ..NodeData::default()
    })
    .await;
    let id = recovery::paideia_proxy_box_id(&test.state, &b.transaction_id.to_string())
        .await
        .unwrap();
    assert_eq!(id, b.box_id().to_string());
    let check = recovery::check_paideia_proxy(&test.state, &id)
        .await
        .unwrap();
    assert!(!check.executor.valid);
    assert!(check.executor.message.contains("state box not found"));
    assert!(check.refund.valid);
    let data = test.data.lock().unwrap();
    assert!(
        !data
            .requests
            .iter()
            .any(|path| path.starts_with("/blockchain/box/byId/")),
        "pending box metadata must come from its own creation fields"
    );
    assert!(!data
        .requests
        .iter()
        .any(|path| path.starts_with("/blockchain/transaction/byId/")));
    assert!(
        !data.requests.iter().any(|path| path == "/transactions"),
        "checks cannot broadcast"
    );
}

#[tokio::test]
async fn unavailable_mempool_cannot_present_spent_confirmed_proxies_as_pending_recoveries() {
    let b = proxy("08", OWNER, 1_000_000);
    let test = node(NodeData {
        confirmed: vec![serde_json::to_value(&b).unwrap()],
        mempool_error: true,
        ..NodeData::default()
    })
    .await;
    let error = recovery::scan_pending_paideia_recoveries(&test.state)
        .await
        .unwrap_err();
    assert!(error.contains("mempool"));
}

#[tokio::test]
async fn discovered_legacy_proxy_at_nonzero_index_can_check_refund_by_its_box_id() {
    let original = proxy("0b", OWNER, 1_000_000);
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy.as_object_mut().unwrap().remove("boxId");
    legacy["index"] = 3.into();
    let legacy: ErgoBox = serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.index, 3);
    assert_ne!(legacy.box_id(), original.box_id());
    let legacy_id = legacy.box_id().to_string();
    let legacy_json = serde_json::to_value(&legacy).unwrap();
    let test = node(NodeData {
        confirmed: vec![legacy_json.clone()],
        live_proxy: Some(legacy_json),
        ..NodeData::default()
    })
    .await;

    let scan = recovery::scan_pending_paideia_recoveries(&test.state)
        .await
        .unwrap();
    assert_eq!(scan.recoveries.len(), 1);
    let found = &scan.recoveries[0];
    assert!(found.confirmed);
    assert_eq!(found.proxy_box_id, legacy_id);
    assert_eq!(found.creation_tx_id, legacy.transaction_id.to_string());
    let check = recovery::check_paideia_proxy(&test.state, &found.proxy_box_id)
        .await
        .unwrap();
    assert_eq!(check.proxy_box_id, legacy_id);
    assert!(!check.executor.valid);
    assert!(check.refund.valid);
    assert_eq!(check.refund.message, "accepted-refund");

    let data = test.data.lock().unwrap();
    assert!(data
        .requests
        .contains(&format!("/utxo/withPool/byId/{legacy_id}")));
    assert!(!data.requests.iter().any(|path| {
        path.starts_with("/transactions/unconfirmed/byTransactionId/")
            || path.starts_with("/blockchain/transaction/byId/")
    }));
    assert!(
        !data.requests.iter().any(|path| path == "/transactions"),
        "refund checks must not broadcast"
    );
}

#[tokio::test]
async fn scans_later_proxy_pages_and_all_wallet_recipients() {
    let mut boxes = Vec::new();
    for seed in 0..500 {
        let mut b = serde_json::to_value(proxy("09", OTHER_OWNER, 1_000_000)).unwrap();
        b.as_object_mut().unwrap().remove("boxId");
        b["transactionId"] = format!("{seed:064x}").into();
        boxes.push(b);
    }
    let older = proxy("0a", OWNER, 1_000_001);
    boxes.push(serde_json::to_value(&older).unwrap());
    let test = node(NodeData {
        confirmed: boxes,
        ..NodeData::default()
    })
    .await;
    let scan = recovery::scan_pending_paideia_recoveries(&test.state)
        .await
        .unwrap();
    assert_eq!(scan.pages_fetched, 2);
    assert_eq!(scan.recoveries.len(), 1);
    assert_eq!(scan.recoveries[0].proxy_box_id, older.box_id().to_string());

    let primary = test.state.wallet().await.unwrap().address;
    let other_tree = ErgoTree::sigma_parse_bytes(&hex::decode(OTHER_OWNER).unwrap()).unwrap();
    let other_address = AddressEncoder::new(NetworkPrefix::Mainnet)
        .address_to_str(&Address::recreate_from_ergo_tree(&other_tree).unwrap());
    test.state
        .set_wallet_addresses(primary, vec![other_address])
        .await
        .unwrap();
    let scan = recovery::scan_pending_paideia_recoveries(&test.state)
        .await
        .unwrap();
    assert_eq!(scan.recoveries.len(), 501);
    assert!(!scan.hit_page_limit);
}
