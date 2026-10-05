//! Execute the actual Paideia scripts with proxy/state outputs created at the
//! current height, before any block confirmation. Position registers are from
//! the historical mainnet transaction used by the byte-exact builder tests.
use ergo_lib::chain::ergo_state_context::{ErgoStateContext, Headers};
use ergo_lib::chain::parameters::Parameters;
use ergo_lib::chain::transaction::reduced::ReducedTransaction;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::SigmaBoolean;
use ergo_tx::{Eip12InputBox, Eip12UnsignedTx};
use serde_json::json;
use stake_recovery::{
    build_paideia_executor_tx, build_paideia_proxy_tx, build_paideia_refund_tx, parse_stake_box,
    StakeStateSnapshot, PAIDEIA,
};

const OWNER: &str = "0008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6";
const KEY: &str = "83ecb7dd522bd78a0132dbd0fc329f00aa721283ea15451f31242b95aed91f1e";
const HEIGHT: i32 = 1_882_943;

fn input(b: &ErgoBox) -> Eip12InputBox {
    Eip12InputBox::from_ergo_box(b, b.transaction_id.to_string(), b.index)
}

fn state_box() -> ErgoBox {
    serde_json::from_str(include_str!("fixtures/paideia_state.json")).unwrap()
}

fn stake_box(key: &str) -> ErgoBox {
    let mut b: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/paideia_stake.json")).unwrap();
    if key != KEY {
        b.as_object_mut().unwrap().remove("boxId");
        b["transactionId"] = key.into();
        b["additionalRegisters"]["R5"] = format!("0e20{key}").into();
    }
    serde_json::from_value(b).unwrap()
}

#[test]
fn registered_paideia_contracts_match_mainnet_fixtures_and_addresses() {
    use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    for (b, tree, address) in [
        (
            state_box(),
            PAIDEIA.stake_state_ergo_tree,
            PAIDEIA.stake_state_address,
        ),
        (
            stake_box(KEY),
            PAIDEIA.stake_box_ergo_tree,
            PAIDEIA.stake_box_address,
        ),
    ] {
        let bytes = b.ergo_tree.sigma_serialize_bytes().unwrap();
        assert_eq!(hex::encode(&bytes), tree);
        assert_eq!(encoder.address_to_str(&Address::P2S(bytes)), address);
    }
}

fn wallet(key: &str) -> Eip12InputBox {
    input(
        &serde_json::from_value(json!({
            "transactionId":key,"index":1,"value":200_000_000,"ergoTree":OWNER,
            "creationHeight":HEIGHT,"additionalRegisters":{},
            "assets":[{"tokenId":key,"amount":1}]
        }))
        .unwrap(),
    )
}

fn snapshot(b: &ErgoBox) -> StakeStateSnapshot {
    StakeStateSnapshot {
        protocol: "Paideia".into(),
        state_box_id: b.box_id().to_string(),
        state_box_value_nano: 1_000_000,
        total_staked_raw: 509_068_736_638,
        checkpoint: 790,
        num_stakers: 474,
        last_checkpoint_ts: 1_722_513_600_000,
        cycle_duration_ms: 86_400_000,
        stake_token_amount: 999_999_999_526,
    }
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

fn output(tx: &ReducedTransaction, index: usize) -> ErgoBox {
    ErgoBox::from_box_candidate(
        &tx.unsigned_tx.output_candidates.as_vec()[index],
        tx.unsigned_tx.id(),
        index as u16,
    )
    .unwrap()
}

fn create_proxy(key: &str, state: &StakeStateSnapshot) -> Eip12InputBox {
    let stake = parse_stake_box(&stake_box(key), &PAIDEIA).unwrap();
    let tx = build_paideia_proxy_tx(&stake, state, &[wallet(key)], OWNER, HEIGHT).unwrap();
    input(&output(&reduce(&tx), 0))
}

fn assert_permissionless(tx: &Eip12UnsignedTx) -> ReducedTransaction {
    let reduced = reduce(tx);
    for input in reduced.reduced_inputs().as_vec() {
        assert_eq!(input.sigma_prop, SigmaBoolean::TrivialProp(true));
    }
    reduced
}

#[test]
fn newly_created_proxy_can_pay_or_refund_without_a_block_wait() {
    let state_box = state_box();
    let state = snapshot(&state_box);
    let stake_box = stake_box(KEY);
    let stake = parse_stake_box(&stake_box, &PAIDEIA).unwrap();
    let proxy = create_proxy(KEY, &state);
    assert_eq!(proxy.creation_height, HEIGHT);
    let executor = build_paideia_executor_tx(
        &input(&state_box),
        &state,
        &input(&stake_box),
        &stake,
        &proxy,
        OWNER,
        HEIGHT,
    )
    .unwrap();
    assert_permissionless(&executor);
    let refund = build_paideia_refund_tx(&proxy, HEIGHT).unwrap();
    assert_permissionless(&refund);
}

#[test]
fn consecutive_paideia_claims_accept_the_unconfirmed_replacement_state() {
    let original = state_box();
    let mut state = snapshot(&original);
    let first_stake = stake_box(KEY);
    let first = build_paideia_executor_tx(
        &input(&original),
        &state,
        &input(&first_stake),
        &parse_stake_box(&first_stake, &PAIDEIA).unwrap(),
        &create_proxy(KEY, &state),
        OWNER,
        HEIGHT,
    )
    .unwrap();
    let first_reduced = assert_permissionless(&first);
    let replacement = output(&first_reduced, 0);
    state.state_box_id = replacement.box_id().to_string();
    state.total_staked_raw -= 850_495_800;
    state.num_stakers -= 1;
    state.stake_token_amount += 1;
    let second_key = "ab".repeat(32);
    let second_stake = stake_box(&second_key);
    let second = build_paideia_executor_tx(
        &input(&replacement),
        &state,
        &input(&second_stake),
        &parse_stake_box(&second_stake, &PAIDEIA).unwrap(),
        &create_proxy(&second_key, &state),
        OWNER,
        HEIGHT,
    )
    .unwrap();
    assert_eq!(second.inputs[0].box_id, replacement.box_id().to_string());
    assert!(second.inputs.iter().all(|input| !first
        .inputs
        .iter()
        .any(|previous| previous.box_id == input.box_id)));
    assert_permissionless(&second);
}
