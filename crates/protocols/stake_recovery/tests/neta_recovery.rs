use ergo_lib::chain::ergo_state_context::{ErgoStateContext, Headers};
use ergo_lib::chain::parameters::Parameters;
use ergo_lib::chain::transaction::reduced::ReducedTransaction;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::SigmaBoolean;
use ergo_tx::{Eip12Asset, Eip12InputBox, Eip12UnsignedTx};
use stake_recovery::{build_recovery_tx_eip12, parse_stake_box, StakeStateSnapshot, NETA};

const KEY: &str = "12fb8192304383f32a5eb880b3e32536036e9ef9150137555ebdde25d03943d0";
const OWNER: &str = "0008cd039d3e8d055631b2926120f2f1cb69709c1da1f52077358a319a747f02d83253fb";

fn fixtures() -> (
    Eip12InputBox,
    StakeStateSnapshot,
    Eip12InputBox,
    stake_recovery::RecoverableStake,
    Vec<Eip12InputBox>,
) {
    let state_box: ErgoBox =
        serde_json::from_str(include_str!("fixtures/neta_state.json")).unwrap();
    let stake_box: ErgoBox =
        serde_json::from_str(include_str!("fixtures/neta_stake.json")).unwrap();
    let stake = parse_stake_box(&stake_box, &NETA).unwrap();
    let state = StakeStateSnapshot {
        protocol: "NETA".into(),
        state_box_id: state_box.box_id().to_string(),
        state_box_value_nano: 1_000_000,
        total_staked_raw: 149_483_263_295_697,
        checkpoint: 182,
        num_stakers: 495,
        last_checkpoint_ts: 1_677_574_800_000,
        cycle_duration_ms: 86_400_000,
        stake_token_amount: 999_999_999_505,
    };
    let input =
        |b: &ErgoBox| Eip12InputBox::from_ergo_box(b, b.transaction_id.to_string(), b.index);
    // The supplied NFT's real wallet box is only 39,960 nanoERG, so recovery
    // must select a top-up and return the key in a funded change output.
    let key_box: ErgoBox = serde_json::from_value(serde_json::json!({
        "transactionId": "aea34997534e7d2da5231fac39dfb0befabf3fc4bf7952a25afc1f47b3d9aaf3",
        "index": 0, "value": 39960, "ergoTree": OWNER,
        "creationHeight": 1879156, "additionalRegisters": {},
        "assets": [{"tokenId": KEY, "amount": 1}]
    }))
    .unwrap();
    let top_up: ErgoBox = serde_json::from_value(serde_json::json!({
        "transactionId": "00".repeat(32), "index": 0, "value": 10_000_000,
        "ergoTree": OWNER, "creationHeight": 1879156,
        "additionalRegisters": {}, "assets": []
    }))
    .unwrap();
    (
        input(&state_box),
        state,
        input(&stake_box),
        stake,
        vec![input(&key_box), input(&top_up)],
    )
}

fn reduce(tx: &Eip12UnsignedTx) -> ReducedTransaction {
    let headers: Headers = serde_json::from_str(include_str!("fixtures/headers.json")).unwrap();
    let context = ErgoStateContext::new(headers[0].clone().into(), headers, Parameters::default());
    let boxes = tx
        .inputs
        .iter()
        .map(|b| serde_json::from_value(serde_json::to_value(b).unwrap()).unwrap())
        .collect();
    let bytes = ergopay_core::reduce_transaction_with_context(tx, boxes, vec![], &context).unwrap();
    ReducedTransaction::sigma_parse_bytes(&bytes).unwrap()
}

#[test]
fn supplied_neta_key_recovers_with_real_contract_scripts() {
    let (state_box, state, stake_box, stake, wallet) = fixtures();
    assert_eq!(stake.stake_key_id, KEY);
    assert_eq!(stake.reward_amount_display, "2386.034991");
    assert_eq!(state_box.ergo_tree, NETA.stake_state_ergo_tree);
    assert_eq!(stake_box.ergo_tree, NETA.stake_box_ergo_tree);
    let tx = build_recovery_tx_eip12(
        &state_box, &state, &stake_box, &stake, &wallet, OWNER, 1882943,
    )
    .unwrap();
    assert_eq!(tx.inputs.len(), 4, "small key box requires top-up");
    assert_eq!(tx.outputs[1].assets[0].token_id, NETA.reward_token);
    assert_eq!(tx.outputs[1].assets[0].amount, "2386034991");
    assert_eq!(tx.outputs[1].ergo_tree, OWNER);
    assert_eq!(tx.outputs[2].assets[0].token_id, KEY);
    assert_eq!(tx.outputs[2].assets[0].amount, "1");
    assert_eq!(tx.outputs[2].ergo_tree, OWNER);
    assert_eq!(tx.outputs[0].assets[1].amount, "999999999506");
    assert_eq!(
        tx.inputs
            .iter()
            .map(|b| b.value.parse::<u64>().unwrap())
            .sum::<u64>(),
        tx.outputs
            .iter()
            .map(|b| b.value.parse::<u64>().unwrap())
            .sum::<u64>()
    );

    let reduced = reduce(&tx);
    assert_eq!(
        reduced.reduced_inputs().as_vec()[0].sigma_prop,
        SigmaBoolean::TrivialProp(true),
        "NETA state must authorize unstake"
    );
    assert_eq!(
        reduced.reduced_inputs().as_vec()[1].sigma_prop,
        SigmaBoolean::TrivialProp(true),
        "NETA stake must authorize unstake"
    );
    assert!(
        !matches!(
            reduced.reduced_inputs().as_vec()[2].sigma_prop,
            SigmaBoolean::TrivialProp(_)
        ),
        "wallet signature is still required"
    );

    // Verify the state script rejects a payout that removes even one NETA unit.
    let mut short_payout = tx;
    short_payout.outputs[1].assets[0].amount = "2386034990".into();
    assert_eq!(
        reduce(&short_payout).reduced_inputs().as_vec()[0].sigma_prop,
        SigmaBoolean::TrivialProp(false)
    );
}

#[test]
fn neta_rejects_stale_checkpoint_and_wrong_key_token_position() {
    let (state_box, mut state, stake_box, stake, mut wallet) = fixtures();
    state.checkpoint += 1;
    let err = build_recovery_tx_eip12(
        &state_box, &state, &stake_box, &stake, &wallet, OWNER, 1882943,
    )
    .unwrap_err();
    assert!(err.to_string().contains("checkpoint"));
    state.checkpoint -= 1;
    wallet[0]
        .assets
        .insert(0, Eip12Asset::new("ab".repeat(32), 1));
    let err = build_recovery_tx_eip12(
        &state_box, &state, &stake_box, &stake, &wallet, OWNER, 1882943,
    )
    .unwrap_err();
    assert!(err.to_string().contains("first token"));
}
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::{json, Value};
use stake_recovery::{discover_recoverable_stakes, fetch_stake_state, find_stake_box_by_key};
use std::sync::{Arc, Mutex};

struct MockData {
    transactions: Vec<Value>,
    confirmed_state: Value,
    confirmed_stakes: Vec<Value>,
    mempool_error: Option<StatusCode>,
}

struct MockNode {
    client: ergo_node_client::NodeClient,
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
        .find_map(|s| s.strip_prefix("offset="))
        .unwrap_or("0")
        .parse()
        .unwrap();
    let body = axum::body::to_bytes(request.into_body(), 32_768)
        .await
        .unwrap();
    let data = data.lock().unwrap();
    match path.as_str() {
        "/info" => Json(json!({"fullHeight": 1882943, "extraIndex": true})).into_response(),
        "/blockchain/indexedHeight" => {
            Json(json!({"indexedHeight":1882943, "fullHeight":1882943})).into_response()
        }
        "/transactions/unconfirmed" | "/transactions/unconfirmed/byErgoTree" => {
            if let Some(status) = data.mempool_error {
                return (status, Json(json!({"error":"mempool unavailable"}))).into_response();
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
        "/blockchain/box/unspent/byAddress" => {
            let address: String = serde_json::from_slice(&body).unwrap();
            let boxes = if address == NETA.stake_state_address {
                vec![data.confirmed_state.clone()]
            } else if address == NETA.stake_box_address {
                data.confirmed_stakes.clone()
            } else {
                vec![]
            };
            Json(boxes).into_response()
        }
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unexpected confirmed-only lookup"})),
        )
            .into_response(),
    }
}

async fn mock_node(transactions: Vec<Value>, confirmed_stakes: Vec<Value>) -> MockNode {
    let data = Arc::new(Mutex::new(MockData {
        transactions,
        confirmed_state: serde_json::from_str(include_str!("fixtures/neta_state.json")).unwrap(),
        confirmed_stakes,
        mempool_error: None,
    }));
    let app = Router::new()
        .fallback(mock_response)
        .with_state(data.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = ergo_node_client::NodeClient::new(citadel_core::NodeConfig {
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

fn mempool_transaction(tx: &Eip12UnsignedTx) -> (Value, Vec<Eip12InputBox>) {
    let reduced = reduce(tx);
    for i in 0..2 {
        assert_eq!(
            reduced.reduced_inputs().as_vec()[i].sigma_prop,
            SigmaBoolean::TrivialProp(true)
        );
    }
    let unsigned = reduced.unsigned_tx;
    let outputs: Vec<ErgoBox> = unsigned
        .output_candidates
        .iter()
        .enumerate()
        .map(|(i, b)| ErgoBox::from_box_candidate(b, unsigned.id(), i as u16).unwrap())
        .collect();
    let json = json!({"id": unsigned.id().to_string(), "inputs": tx.inputs.iter().map(|b| json!({"boxId": b.box_id})).collect::<Vec<_>>(), "outputs": outputs});
    let inputs = outputs
        .iter()
        .map(|b| Eip12InputBox::from_ergo_box(b, b.transaction_id.to_string(), b.index))
        .collect();
    (json, inputs)
}

#[tokio::test]
async fn consecutive_neta_claims_use_unconfirmed_state_and_wallet_change() {
    let (state_box, state, stake_box, stake, wallet) = fixtures();
    let tx1 = build_recovery_tx_eip12(
        &state_box, &state, &stake_box, &stake, &wallet, OWNER, 1882943,
    )
    .unwrap();
    let (pending1, outputs1) = mempool_transaction(&tx1);
    // A second position uses the same actual scripts with a distinct stake key.
    let key2 = "22".repeat(32);
    let mut second: Value = serde_json::from_str(include_str!("fixtures/neta_stake.json")).unwrap();
    second.as_object_mut().unwrap().remove("boxId");
    second["transactionId"] = "33".repeat(32).into();
    second["additionalRegisters"]["R5"] = format!("0e20{key2}").into();
    let second: ErgoBox = serde_json::from_value(second).unwrap();
    let first: Value = serde_json::from_str(include_str!("fixtures/neta_stake.json")).unwrap();
    let node = mock_node(
        vec![pending1.clone()],
        vec![first, serde_json::to_value(&second).unwrap()],
    )
    .await;

    let (next_state, snapshot) = fetch_stake_state(&node.client, &NETA).await.unwrap();
    assert_eq!(next_state.box_id().to_string(), outputs1[0].box_id);
    assert_eq!(snapshot.num_stakers, state.num_stakers - 1);
    assert_eq!(
        snapshot.total_staked_raw,
        state.total_staked_raw - stake.reward_amount_raw
    );
    assert!(
        find_stake_box_by_key(&node.client, KEY).await.is_err(),
        "already-pending stake must not be redeemable again"
    );
    let (cfg, stake2) = find_stake_box_by_key(&node.client, &key2).await.unwrap();
    assert_eq!(cfg.name, "NETA");
    let scan = discover_recoverable_stakes(&node.client, &[KEY.into(), key2.clone()])
        .await
        .unwrap();
    assert_eq!(scan.stakes.len(), 1);
    assert_eq!(scan.stakes[0].stake_key_id, key2);

    let mut key_box = serde_json::to_value(&wallet[0]).unwrap();
    key_box.as_object_mut().unwrap().remove("boxId");
    key_box["transactionId"] = "44".repeat(32).into();
    key_box["assets"][0]["tokenId"] = key2.clone().into();
    let key_box: ErgoBox = serde_json::from_value(key_box).unwrap();
    let state_input = Eip12InputBox::from_ergo_box(
        &next_state,
        next_state.transaction_id.to_string(),
        next_state.index,
    );
    let stake_input =
        Eip12InputBox::from_ergo_box(&stake2, stake2.transaction_id.to_string(), stake2.index);
    let second_wallet = vec![
        Eip12InputBox::from_ergo_box(&key_box, key_box.transaction_id.to_string(), key_box.index),
        outputs1[2].clone(),
    ];
    let tx2 = build_recovery_tx_eip12(
        &state_input,
        &snapshot,
        &stake_input,
        &parse_stake_box(&stake2, &NETA).unwrap(),
        &second_wallet,
        OWNER,
        1882943,
    )
    .unwrap();
    assert_eq!(tx2.inputs[0].box_id, outputs1[0].box_id);
    assert!(tx2.inputs.iter().any(|b| b.box_id == outputs1[2].box_id));
    assert!(
        tx2.inputs
            .iter()
            .all(|b| tx1.inputs.iter().all(|old| b.box_id != old.box_id)),
        "no double-spent inputs"
    );
    let (pending2, outputs2) = mempool_transaction(&tx2);
    // Deliberately unsorted, with a duplicate: never choose a spent intermediate.
    node.data.lock().unwrap().transactions = vec![pending2, pending1.clone(), pending1];
    let (latest, snapshot2) = fetch_stake_state(&node.client, &NETA).await.unwrap();
    assert_eq!(latest.box_id().to_string(), outputs2[0].box_id);
    assert_eq!(snapshot2.num_stakers, state.num_stakers - 2);
    let scan = discover_recoverable_stakes(&node.client, &[KEY.into(), key2])
        .await
        .unwrap();
    assert!(scan.stakes.is_empty());
}

#[tokio::test]
async fn mempool_pagination_and_errors_never_select_stale_confirmed_state() {
    let (state_box, state, stake_box, stake, wallet) = fixtures();
    let tx = build_recovery_tx_eip12(
        &state_box, &state, &stake_box, &stake, &wallet, OWNER, 1882943,
    )
    .unwrap();
    let (pending, outputs) = mempool_transaction(&tx);
    let mut transactions: Vec<Value> = (0..100)
        .map(|i| json!({"id":format!("{i:064x}"), "inputs":[], "outputs":[]}))
        .collect();
    transactions.push(pending);
    let node = mock_node(transactions, vec![]).await;
    let (latest, _) = fetch_stake_state(&node.client, &NETA).await.unwrap();
    assert_eq!(latest.box_id().to_string(), outputs[0].box_id);
    assert_eq!(
        node.client
            .get_unconfirmed_by_ergo_tree(NETA.stake_state_ergo_tree)
            .await
            .unwrap()
            .len(),
        101
    );
    // Both HTTP failures and JSON error objects with 200 status must fail closed.
    for status in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        node.data.lock().unwrap().mempool_error = Some(status);
        let error = fetch_stake_state(&node.client, &NETA).await.unwrap_err();
        assert!(error.to_string().contains("mempool"));
    }
}
