use std::collections::HashMap;

use citadel_core::TxError;
use ergo_tx::{Eip12Asset, Eip12InputBox};

use super::*;
use crate::fetch::DexyLpTxContext;

const DEXY_TOKEN_ID: &str =
    "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
const LP_TOKEN_ID: &str =
    "cf74432b2d3ab8a1a934b6326a1004e1a19aec7b357c57209018c4aa35226246";
const LP_MINT_NFT_ID: &str =
    "19b8281b141d19c5b3843a4a77e616d6df05f601e5908159b1eaf3d9da20e664";
const LP_REDEEM_NFT_ID: &str =
    "08c47eef5e782f146cae5e8cfb5e9d26b18442f82f3c5808b1563b6e3b23f729";
const ORACLE_NFT_ID: &str =
    "3c45f29a5165b030fdb5eaf5d81f8108f9d8f507b31487dd51f4ae08fe07cf4a";

const INITIAL_LP: i64 = 100_000_000_000; // Gold initial LP

fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
    use ergo_lib::ergotree_ir::chain::ergo_box::{
        box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
    };
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

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

fn create_deposit_context(
    lp_erg: i64,
    lp_dexy: i64,
    lp_token_reserves: i64,
) -> DexyLpTxContext {
    let lp_input = Eip12InputBox {
        box_id: "lp_box_id".to_string(),
        transaction_id: "lp_tx_id".to_string(),
        index: 0,
        value: lp_erg.to_string(),
        ergo_tree: "lp_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset::new(LP_NFT_ID, 1),
            Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
            Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
        ],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let action_input = Eip12InputBox {
        box_id: "mint_box_id".to_string(),
        transaction_id: "mint_tx_id".to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "mint_ergo_tree_hex".to_string(),
        assets: vec![Eip12Asset::new(LP_MINT_NFT_ID, 1)],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let dummy_box = create_dummy_ergo_box();

    DexyLpTxContext {
        lp_input,
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_token_reserves,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy_box.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        action_input,
        action_erg_value: 1_000_000,
        action_ergo_tree: "mint_ergo_tree_hex".to_string(),
        action_box: dummy_box,
        action_tokens: vec![(LP_MINT_NFT_ID.to_string(), 1)],
        oracle_data_input: None,
        oracle_rate_nano: None,
    }
}

fn create_redeem_context(
    lp_erg: i64,
    lp_dexy: i64,
    lp_token_reserves: i64,
    oracle_rate_nano: i64,
) -> DexyLpTxContext {
    use ergo_tx::Eip12DataInputBox;

    let lp_input = Eip12InputBox {
        box_id: "lp_box_id".to_string(),
        transaction_id: "lp_tx_id".to_string(),
        index: 0,
        value: lp_erg.to_string(),
        ergo_tree: "lp_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset::new(LP_NFT_ID, 1),
            Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
            Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
        ],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let action_input = Eip12InputBox {
        box_id: "redeem_box_id".to_string(),
        transaction_id: "redeem_tx_id".to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "redeem_ergo_tree_hex".to_string(),
        assets: vec![Eip12Asset::new(LP_REDEEM_NFT_ID, 1)],
        creation_height: 100000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };

    let oracle_data_input = Eip12DataInputBox {
        box_id: "oracle_box_id".to_string(),
        transaction_id: "oracle_tx_id".to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "oracle_ergo_tree_hex".to_string(),
        assets: vec![Eip12Asset::new(ORACLE_NFT_ID, 1)],
        creation_height: 100000,
        additional_registers: HashMap::new(),
    };

    let dummy_box = create_dummy_ergo_box();

    DexyLpTxContext {
        lp_input,
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_token_reserves,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy_box.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        action_input,
        action_erg_value: 1_000_000,
        action_ergo_tree: "redeem_ergo_tree_hex".to_string(),
        action_box: dummy_box,
        action_tokens: vec![(LP_REDEEM_NFT_ID.to_string(), 1)],
        oracle_data_input: Some(oracle_data_input),
        oracle_rate_nano: Some(oracle_rate_nano),
    }
}


#[test]
fn test_lp_deposit_rejects_zero_erg() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
    let request = LpDepositRequest {
        variant: DexyVariant::Gold,
        deposit_erg: 0,
        deposit_dexy: 100,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            100_000_000_000,
            vec![(DEXY_TOKEN_ID, 1000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    };

    let result =
        build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("ERG"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_lp_deposit_rejects_zero_dexy() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
    let request = LpDepositRequest {
        variant: DexyVariant::Gold,
        deposit_erg: 10_000_000_000,
        deposit_dexy: 0,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            100_000_000_000,
            vec![(DEXY_TOKEN_ID, 1000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    };

    let result =
        build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("Dexy"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_lp_deposit_builds_correctly() {
    no_citadel_fee(|| {
        let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
        let request = LpDepositRequest {
            variant: DexyVariant::Gold,
            deposit_erg: 10_000_000_000,
            deposit_dexy: 5_000,
            user_address: "user_addr".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![create_test_input(
                100_000_000_000, // 100 ERG
                vec![(DEXY_TOKEN_ID, 10_000)],
            )],
            current_height: 100000,
            recipient_ergo_tree: None,
        };

        let result =
            build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
        assert!(result.is_ok(), "Build failed: {:?}", result.err());

        let build = result.unwrap();
        let tx = &build.unsigned_tx;

        assert_eq!(tx.inputs.len(), 3);
        assert_eq!(tx.inputs[0].box_id, "lp_box_id");
        assert_eq!(tx.inputs[1].box_id, "mint_box_id");
        assert_eq!(tx.data_inputs.len(), 0);
        assert!(
            tx.outputs.len() >= 3,
            "Expected at least 3 outputs, got {}",
            tx.outputs.len()
        );

        assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
        let lp_erg_out: i64 = tx.outputs[0].value.parse().unwrap();
        assert!(lp_erg_out > 1_000_000_000_000, "LP ERG should increase after deposit");
        assert_eq!(tx.outputs[0].assets.len(), 3);
        assert_eq!(tx.outputs[0].assets[0].token_id, LP_NFT_ID);
        assert_eq!(tx.outputs[0].assets[1].token_id, LP_TOKEN_ID);
        assert_eq!(tx.outputs[0].assets[2].token_id, DEXY_TOKEN_ID);
        let new_lp_reserves: i64 = tx.outputs[0].assets[1].amount.parse().unwrap();
        assert!(new_lp_reserves < 99_900_000_000, "LP token reserves should decrease");
        assert_eq!(tx.outputs[1].ergo_tree, "mint_ergo_tree_hex");
        assert_eq!(tx.outputs[1].value, "1000000");
        assert_eq!(tx.outputs[1].assets.len(), 1);
        assert_eq!(tx.outputs[1].assets[0].token_id, LP_MINT_NFT_ID);

        let user_output = &tx.outputs[2];
        assert_eq!(user_output.ergo_tree, "user_ergo_tree");
        assert!(
            user_output.assets.iter().any(|a| a.token_id == LP_TOKEN_ID),
            "User should receive LP tokens"
        );
        let lp_out: i64 = user_output
            .assets
            .iter()
            .find(|a| a.token_id == LP_TOKEN_ID)
            .unwrap()
            .amount
            .parse()
            .unwrap();
        assert!(lp_out > 0, "User should receive positive LP tokens");

        assert!(build.summary.action.starts_with("lp_deposit"));
        assert!(build.summary.erg_amount > 0);
        assert!(build.summary.dexy_amount > 0);
        assert!(build.summary.lp_tokens > 0);
            });
}

#[test]
fn test_lp_deposit_with_recipient() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

    let request = LpDepositRequest {
        variant: DexyVariant::Gold,
        deposit_erg: 10_000_000_000,
        deposit_dexy: 5_000,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            100_000_000_000,
            vec![(DEXY_TOKEN_ID, 10_000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: Some("recipient_ergo_tree".to_string()),
    };

    let result =
        build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_ok(), "Build failed: {:?}", result.err());

    let tx = &result.unwrap().unsigned_tx;
    assert_eq!(tx.outputs[2].ergo_tree, "recipient_ergo_tree");
}


#[test]
fn test_lp_redeem_rejects_zero_lp() {
    // Oracle rate: 1,000,000,000,000 raw nanoERG/kg (for Gold, divisor = 1M -> 1,000,000 nanoERG/mg)
    // LP rate: 1,000,000,000,000 / 500,000 = 2,000,000 nanoERG/token
    // Oracle adjusted: 1,000,000 nanoERG/token
    // can_redeem: lp_rate(2M) > oracle_adjusted(1M) * 98/100 -> true
    let ctx = create_redeem_context(
        1_000_000_000_000,
        500_000,
        99_900_000_000,
        1_000_000_000_000, // raw oracle rate (nanoERG per kg)
    );

    let request = LpRedeemRequest {
        variant: DexyVariant::Gold,
        lp_to_burn: 0,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            10_000_000_000,
            vec![(LP_TOKEN_ID, 1_000_000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    };

    let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("positive"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_lp_redeem_blocked_by_oracle_gate() {
    // Set oracle rate very high so LP rate < 98% of oracle
    // LP rate: 1,000,000,000,000 / 500,000 = 2,000,000 nanoERG/token
    // Oracle raw: 3,000,000,000,000 -> adjusted: 3,000,000 nanoERG/token
    // can_redeem: lp_rate(2M) > oracle_adjusted(3M) * 98/100 = 2.94M -> false
    let ctx = create_redeem_context(
        1_000_000_000_000,
        500_000,
        99_900_000_000,
        3_000_000_000_000, // High oracle rate -> LP depeg -> blocked
    );

    let request = LpRedeemRequest {
        variant: DexyVariant::Gold,
        lp_to_burn: 1_000_000,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            10_000_000_000,
            vec![(LP_TOKEN_ID, 1_000_000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    };

    let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("depeg protection"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_lp_redeem_builds_correctly() {
    no_citadel_fee(|| {
        // Pool: 1000 ERG, 500K Dexy, 99.9B LP tokens reserved (100M circulating)
        // Oracle rate (raw): 1T nanoERG/kg -> adjusted: 1M nanoERG/mg
        // LP rate: 1T / 500K = 2M nanoERG/token
        // can_redeem: 2M > 1M * 98/100 = 980K -> true
        let ctx = create_redeem_context(
            1_000_000_000_000,
            500_000,
            99_900_000_000,
            1_000_000_000_000,
        );

        let request = LpRedeemRequest {
            variant: DexyVariant::Gold,
            lp_to_burn: 1_000_000,
            user_address: "user_addr".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![create_test_input(
                10_000_000_000, // 10 ERG for fees
                vec![(LP_TOKEN_ID, 2_000_000)],
            )],
            current_height: 100000,
            recipient_ergo_tree: None,
        };

        let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
        assert!(result.is_ok(), "Build failed: {:?}", result.err());

        let build = result.unwrap();
        let tx = &build.unsigned_tx;

        assert_eq!(tx.inputs.len(), 3);
        assert_eq!(tx.inputs[0].box_id, "lp_box_id");
        assert_eq!(tx.inputs[1].box_id, "redeem_box_id");
        assert_eq!(tx.data_inputs.len(), 1);
        assert_eq!(tx.data_inputs[0].box_id, "oracle_box_id");
        assert!(
            tx.outputs.len() >= 4,
            "Expected at least 4 outputs, got {}",
            tx.outputs.len()
        );

        assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");
        let lp_erg_out: i64 = tx.outputs[0].value.parse().unwrap();
        assert!(lp_erg_out < 1_000_000_000_000, "LP ERG should decrease after redeem");
        let new_lp_reserves: i64 = tx.outputs[0].assets[1].amount.parse().unwrap();
        assert!(new_lp_reserves > 99_900_000_000, "LP token reserves should increase");
        assert_eq!(tx.outputs[1].ergo_tree, "redeem_ergo_tree_hex");
        assert_eq!(tx.outputs[1].value, "1000000");
        assert_eq!(tx.outputs[1].assets.len(), 1);
        assert_eq!(tx.outputs[1].assets[0].token_id, LP_REDEEM_NFT_ID);

        let user_output = &tx.outputs[2];
        assert_eq!(user_output.ergo_tree, "user_ergo_tree");
        let user_erg: i64 = user_output.value.parse().unwrap();
        assert!(user_erg > 0, "User should receive ERG");
        let dexy_asset = user_output
            .assets
            .iter()
            .find(|a| a.token_id == DEXY_TOKEN_ID);
        assert!(dexy_asset.is_some(), "User should receive Dexy tokens");
        let dexy_out: i64 = dexy_asset.unwrap().amount.parse().unwrap();
        assert!(dexy_out > 0, "User should receive positive Dexy tokens");

        assert!(build.summary.action.starts_with("lp_redeem"));
        assert!(build.summary.erg_amount > 0);
        assert!(build.summary.dexy_amount > 0);
        assert_eq!(build.summary.lp_tokens, 1_000_000);
            });
}

#[test]
fn test_lp_redeem_no_oracle_fails() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

    let request = LpRedeemRequest {
        variant: DexyVariant::Gold,
        lp_to_burn: 1_000_000,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            10_000_000_000,
            vec![(LP_TOKEN_ID, 2_000_000)],
        )],
        current_height: 100000,
        recipient_ergo_tree: None,
    };

    let result = build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP);
    assert!(result.is_err());
    match result.unwrap_err() {
        TxError::BuildFailed { message } => {
            assert!(message.contains("Oracle"), "Got: {}", message);
        }
        other => panic!("Expected BuildFailed, got {:?}", other),
    }
}

#[test]
fn test_lp_deposit_summary_serialization() {
    let summary = LpTxSummary {
        action: "lp_deposit_gold".to_string(),
        erg_amount: 10_000_000_000,
        dexy_amount: 5_000,
        lp_tokens: 1_000_000,
        miner_fee_nano: 1_100_000,
        citadel_fee_nano: 0,
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("lp_deposit_gold"));

    let parsed: LpTxSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.action, "lp_deposit_gold");
    assert_eq!(parsed.lp_tokens, 1_000_000);
}

#[test]
fn test_lp_pool_output_preserves_token_order() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

    let output = build_lp_pool_output(
        &ctx,
        1_010_000_000_000, // new ERG
        99_899_000_000,    // new LP token reserves
        505_000,           // new Dexy
        LP_TOKEN_ID,
        DEXY_TOKEN_ID,
        100001,
    );

    assert_eq!(output.assets.len(), 3);
    assert_eq!(output.assets[0].token_id, LP_NFT_ID);
    assert_eq!(output.assets[0].amount, "1");
    assert_eq!(output.assets[1].token_id, LP_TOKEN_ID);
    assert_eq!(output.assets[1].amount, "99899000000");
    assert_eq!(output.assets[2].token_id, DEXY_TOKEN_ID);
    assert_eq!(output.assets[2].amount, "505000");
    assert_eq!(output.value, "1010000000000");
}

#[test]
fn test_action_nft_output_self_preservation() {
    let ctx = create_deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);

    let output = build_action_nft_output(&ctx, 100001);

    assert_eq!(output.value, "1000000");
    assert_eq!(output.ergo_tree, "mint_ergo_tree_hex");
    assert_eq!(output.assets.len(), 1);
    assert_eq!(output.assets[0].token_id, LP_MINT_NFT_ID);
    assert_eq!(output.assets[0].amount, "1");
}
