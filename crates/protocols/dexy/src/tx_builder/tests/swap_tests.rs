use std::collections::HashMap;

use citadel_core::{constants, TxError};
use ergo_tx::{Eip12Asset, Eip12InputBox};

use super::*;
use crate::fetch::DexySwapTxContext;

const DEXY_TOKEN_ID: &str =
    "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
const LP_TOKEN_ID: &str = "lp_token_id_placeholder";
const SWAP_NFT_ID: &str =
    "ff7b7eff3c818f9dc573ca03a723a7f6ed1615bf27980ebd4a6c91986b26f801";

fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
    use ergo_lib::ergotree_ir::chain::ergo_box::{
        box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
    };
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    // Use a minimal P2PK ErgoTree (simplest valid tree)
    let ergo_tree_bytes = base16::decode(
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    )
    .unwrap();
    let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).unwrap();
    let tx_id = TxId::zero();

    ErgoBox::new(
        BoxValue::new(1_000_000).unwrap(),
        ergo_tree,
        None,
        NonMandatoryRegisters::empty(),
        100000,
        tx_id,
        0,
    )
    .unwrap()
}

fn create_test_swap_context(lp_erg: i64, lp_dexy: i64) -> DexySwapTxContext {
    let lp_input = Eip12InputBox {
        box_id: "lp_box_id".to_string(),
        transaction_id: "lp_tx_id".to_string(),
        index: 0,
        value: lp_erg.to_string(),
        ergo_tree: "lp_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset::new(LP_NFT_ID, 1),
            Eip12Asset::new(LP_TOKEN_ID, 9_000_000_000_000_000i64),
            Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
        ],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let swap_input = Eip12InputBox {
        box_id: "swap_box_id".to_string(),
        transaction_id: "swap_tx_id".to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "swap_ergo_tree_hex".to_string(),
        assets: vec![Eip12Asset::new(SWAP_NFT_ID, 1)],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let dummy_box = create_dummy_ergo_box();

    DexySwapTxContext {
        lp_input,
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy_box.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), 9_000_000_000_000_000),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        swap_input,
        swap_erg_value: 1_000_000,
        swap_ergo_tree: "swap_ergo_tree_hex".to_string(),
        swap_box: dummy_box,
        swap_tokens: vec![(SWAP_NFT_ID.to_string(), 1)],
    }
}

fn create_swap_state() -> DexyState {
    create_test_state(10000, true)
}

fn create_erg_to_dexy_request(
    input_amount: i64,
    min_output: i64,
    user_erg: i64,
) -> SwapDexyRequest {
    SwapDexyRequest {
        variant: DexyVariant::Gold,
        direction: SwapDirection::ErgToDexy,
        input_amount,
        min_output,
        user_address: "user_address".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(user_erg, vec![])],
        current_height: 100000,
        recipient_ergo_tree: None,
    }
}

fn create_dexy_to_erg_request(
    input_amount: i64,
    min_output: i64,
    user_erg: i64,
    user_dexy: i64,
) -> SwapDexyRequest {
    SwapDexyRequest {
        variant: DexyVariant::Gold,
        direction: SwapDirection::DexyToErg,
        input_amount,
        min_output,
        user_address: "user_address".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            user_erg,
            vec![(DEXY_TOKEN_ID, user_dexy)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    }
}


#[test]
fn test_build_lp_swap_output_updates_dexy_amount() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

    let output = build_lp_swap_output(
        &ctx,
        1_001_000_000_000, // new ERG (added 1 ERG)
        999_000,           // new Dexy (removed 1000)
        DEXY_TOKEN_ID,
        100001,
    );

    assert_eq!(output.value, "1001000000000");
    assert_eq!(output.ergo_tree, "lp_ergo_tree_hex");
    assert_eq!(output.assets.len(), 3);

    assert_eq!(output.assets[0].token_id, LP_NFT_ID);
    assert_eq!(output.assets[0].amount, "1");

    assert_eq!(output.assets[1].token_id, LP_TOKEN_ID);
    assert_eq!(output.assets[1].amount, "9000000000000000");

    assert_eq!(output.assets[2].token_id, DEXY_TOKEN_ID);
    assert_eq!(output.assets[2].amount, "999000");
}

#[test]
fn test_build_swap_nft_output_preserves_exactly() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

    let output = build_swap_nft_output(&ctx, 100001);

    assert_eq!(output.value, "1000000");
    assert_eq!(output.ergo_tree, "swap_ergo_tree_hex");
    assert_eq!(output.assets.len(), 1);
    assert_eq!(output.assets[0].token_id, SWAP_NFT_ID);
    assert_eq!(output.assets[0].amount, "1");
}


#[test]
fn test_swap_rejects_zero_input() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_erg_to_dexy_request(0, 1, 10_000_000_000);

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        TxError::BuildFailed { message } => {
            assert!(message.contains("positive"), "Got: {}", message);
        }
        _ => panic!("Expected BuildFailed, got {:?}", err),
    }
}

#[test]
fn test_swap_rejects_negative_input() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_erg_to_dexy_request(-100, 1, 10_000_000_000);

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
}

#[test]
fn test_swap_rejects_insufficient_erg() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_erg_to_dexy_request(
        10_000_000_000, // 10 ERG input
        1,
        1_000_000_000, // only 1 ERG available
    );

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("Insufficient ERG"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_swap_rejects_insufficient_dexy_tokens() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_dexy_to_erg_request(
        1000,           // sell 1000 Dexy
        1,              // min output
        10_000_000_000, // user has 10 ERG for fees
        100,            // but only 100 Dexy
    );

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("Insufficient token"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_swap_rejects_slippage_violation() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_erg_to_dexy_request(
        1_000_000_000,   // 1 ERG
        999_999_999,     // impossibly high min output
        100_000_000_000, // 100 ERG available
    );

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("below minimum"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}


#[test]
fn test_erg_to_dexy_swap_builds_correctly() {
    no_citadel_fee(|| {
        let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
        let state = create_swap_state();
        let request = create_erg_to_dexy_request(
            1_000_000_000,   // swap 1 ERG
            1,               // min 1 Dexy output
            100_000_000_000, // 100 ERG available
        );

        let result = build_swap_dexy_tx(&request, &ctx, &state);
        assert!(result.is_ok(), "Build failed: {:?}", result.err());

        let build = result.unwrap();
        let tx = &build.unsigned_tx;

        assert_eq!(tx.inputs.len(), 3);
        assert_eq!(tx.inputs[0].box_id, "lp_box_id");
        assert_eq!(tx.inputs[1].box_id, "swap_box_id");
        assert_eq!(tx.data_inputs.len(), 0);
        assert!(tx.outputs.len() >= 4);
        assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
        assert_eq!(tx.outputs[1].ergo_tree, "swap_ergo_tree_hex");
        assert_eq!(tx.outputs[1].value, "1000000");
        assert_eq!(tx.outputs[2].ergo_tree, "user_ergo_tree");
        assert_eq!(tx.outputs[2].assets.len(), 1);
        assert_eq!(tx.outputs[2].assets[0].token_id, DEXY_TOKEN_ID);
        let user_dexy_out: i64 = tx.outputs[2].assets[0].amount.parse().unwrap();
        assert!(user_dexy_out > 0, "User should receive Dexy tokens");
        assert_eq!(
            tx.outputs.last().unwrap().value,
            constants::TX_FEE_NANO.to_string()
        );

        assert_eq!(build.summary.direction, "erg_to_dexy");
        assert_eq!(build.summary.input_amount, 1_000_000_000);
        assert!(build.summary.output_amount > 0);
        assert_eq!(build.summary.fee_pct, 0.3);
        assert_eq!(build.summary.citadel_fee_nano, 0);
            });
}

#[test]
fn test_dexy_to_erg_swap_builds_correctly() {
    no_citadel_fee(|| {
        let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
        let state = create_swap_state();
        let request = create_dexy_to_erg_request(
            100,            // sell 100 Dexy
            1,              // min 1 nanoERG output
            10_000_000_000, // 10 ERG for fees
            1000,           // have 1000 Dexy
        );

        let result = build_swap_dexy_tx(&request, &ctx, &state);
        assert!(result.is_ok(), "Build failed: {:?}", result.err());

        let build = result.unwrap();
        let tx = &build.unsigned_tx;

        assert_eq!(tx.inputs.len(), 3);
        assert_eq!(tx.outputs.len(), 4);

        let user_output = &tx.outputs[2];
        assert_eq!(user_output.ergo_tree, "user_ergo_tree");
        let user_erg_out: i64 = user_output.value.parse().unwrap();
        assert!(
            user_erg_out > 10_000_000_000,
            "User should receive more ERG than started with"
        );

        let remaining_dexy = user_output
            .assets
            .iter()
            .find(|a| a.token_id == DEXY_TOKEN_ID);
        assert!(remaining_dexy.is_some(), "User should have remaining Dexy");
        assert_eq!(remaining_dexy.unwrap().amount, "900");

        assert_eq!(build.summary.direction, "dexy_to_erg");
        assert_eq!(build.summary.input_amount, 100);
        assert!(build.summary.output_amount > 0);
            });
}

#[test]
fn test_swap_summary_price_impact() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();

    let small_request = create_erg_to_dexy_request(
        1_000_000_000, // 1 ERG (0.1% of pool)
        1,
        100_000_000_000,
    );
    let small_result = build_swap_dexy_tx(&small_request, &ctx, &state).unwrap();

    let large_request = create_erg_to_dexy_request(
        100_000_000_000, // 100 ERG (10% of pool)
        1,
        200_000_000_000,
    );
    let large_result = build_swap_dexy_tx(&large_request, &ctx, &state).unwrap();

    assert!(
        large_result.summary.price_impact_pct > small_result.summary.price_impact_pct,
        "Large swap should have higher price impact ({} vs {})",
        large_result.summary.price_impact_pct,
        small_result.summary.price_impact_pct
    );
}

#[test]
fn test_swap_lp_output_preserves_token_order() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_erg_to_dexy_request(1_000_000_000, 1, 100_000_000_000);

    let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
    let lp_output = &result.unsigned_tx.outputs[0];

    assert_eq!(lp_output.assets.len(), 3);
    assert_eq!(lp_output.assets[0].token_id, LP_NFT_ID);
    assert_eq!(lp_output.assets[1].token_id, LP_TOKEN_ID);
    assert_eq!(lp_output.assets[2].token_id, DEXY_TOKEN_ID);
    assert_eq!(lp_output.assets[0].amount, "1");
    assert_eq!(lp_output.assets[1].amount, "9000000000000000");
}

#[test]
fn test_swap_direction_enum() {
    assert_eq!(SwapDirection::ErgToDexy, SwapDirection::ErgToDexy);
    assert_ne!(SwapDirection::ErgToDexy, SwapDirection::DexyToErg);
}

#[test]
fn test_swap_tx_summary_serialization() {
    let summary = SwapTxSummary {
        direction: "erg_to_dexy".to_string(),
        input_amount: 1_000_000_000,
        output_amount: 997,
        min_output: 990,
        price_impact_pct: 0.1,
        fee_pct: 0.3,
        miner_fee_nano: 1_100_000,
        citadel_fee_nano: 0,
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("erg_to_dexy"));
    assert!(json.contains("1000000000"));

    let parsed: SwapTxSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.direction, "erg_to_dexy");
    assert_eq!(parsed.input_amount, 1_000_000_000);
    assert_eq!(parsed.output_amount, 997);
}

#[test]
fn test_dexy_to_erg_insufficient_erg_for_fees() {
    let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
    let state = create_swap_state();
    let request = create_dexy_to_erg_request(
        100,     // sell 100 Dexy
        1,       // min output
        100_000, // only 0.0001 ERG - not enough for fee + min box
        1000,    // enough Dexy
    );

    let result = build_swap_dexy_tx(&request, &ctx, &state);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("Insufficient ERG"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_erg_to_dexy_change_output_when_needed() {
    no_citadel_fee(|| {
        let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
        let state = create_swap_state();
        let request = create_erg_to_dexy_request(
            1_000_000_000, // swap 1 ERG
            1,
            100_000_000_000, // 100 ERG - lots of change
        );

        let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
        let tx = &result.unsigned_tx;

        assert_eq!(tx.outputs.len(), 5, "Should have change output");
        // [lp, swap_nft, user_out, change, miner_fee]
        let change = &tx.outputs[3];
        assert_eq!(change.ergo_tree, "user_ergo_tree");
        let change_erg: i64 = change.value.parse().unwrap();
        assert!(change_erg >= constants::MIN_BOX_VALUE_NANO);
        assert_eq!(
            tx.outputs[4].value,
            constants::TX_FEE_NANO.to_string()
        );
            });
}
