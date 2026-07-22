use std::collections::HashMap;

use super::common::user_utxo_to_eip12;
use super::*;

const TEST_ADDRESS: &str = "9hY16vzHmmfyVBwKeFGHvb2bMFsG94A1u7To1QWtUokACyFVENQ";

fn sample_utxo(box_id: &str, value: i64, assets: Vec<(String, i64)>) -> UserUtxo {
    UserUtxo {
        box_id: box_id.to_string(),
        tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
        index: 0,
        value,
        ergo_tree: "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f"
            .to_string(),
        assets,
        creation_height: 1000000,
        registers: HashMap::new(),
    }
}

#[test]
fn test_build_error_display() {
    let err = BuildError::InsufficientBalance {
        required: 100,
        available: 50,
    };
    let msg = err.to_string();
    assert!(msg.contains("100"));
    assert!(msg.contains("50"));

    let err = BuildError::InvalidAmount("test".to_string());
    assert!(err.to_string().contains("test"));

    let err = BuildError::PoolNotFound("unknown".to_string());
    assert!(err.to_string().contains("unknown"));

    let err = BuildError::CollateralBoxNotFound("boxid".to_string());
    assert!(err.to_string().contains("boxid"));
}

#[test]
fn test_build_error_codes() {
    assert_eq!(
        BuildError::PoolNotFound("x".to_string()).code(),
        "pool_not_found"
    );
    assert_eq!(
        BuildError::InvalidAmount("x".to_string()).code(),
        "invalid_amount"
    );
    assert_eq!(
        BuildError::InsufficientBalance {
            required: 1,
            available: 0
        }
        .code(),
        "insufficient_balance"
    );
    assert_eq!(
        BuildError::InsufficientTokens {
            token: "x".to_string(),
            required: 1,
            available: 0
        }
        .code(),
        "insufficient_tokens"
    );
}

#[test]
fn test_build_error_status_codes() {
    assert_eq!(
        BuildError::InvalidAmount("x".to_string()).status_code(),
        400
    );
    assert_eq!(
        BuildError::InvalidAddress("x".to_string()).status_code(),
        400
    );
    assert_eq!(
        BuildError::InsufficientBalance {
            required: 1,
            available: 0
        }
        .status_code(),
        422
    );
    assert_eq!(BuildError::PoolNotFound("x".to_string()).status_code(), 404);
    assert_eq!(
        BuildError::ProxyContractMissing("x".to_string()).status_code(),
        503
    );
    assert_eq!(BuildError::TxBuildError("x".to_string()).status_code(), 500);
}

#[test]
fn test_select_erg_inputs_success() {
    let utxos = vec![
        sample_utxo("box1", 1_000_000_000, vec![]), // 1 ERG
        sample_utxo("box2", 2_000_000_000, vec![]), // 2 ERG
        sample_utxo("box3", 500_000_000, vec![]),   // 0.5 ERG
    ];

    // Need 1.5 ERG - should select box2 (2 ERG) first
    let result = select_erg_inputs(&utxos, 1_500_000_000).unwrap();
    assert_eq!(result.boxes.len(), 1);
    assert_eq!(result.total_erg, 2_000_000_000);
}

#[test]
fn test_select_erg_inputs_multiple_boxes() {
    let utxos = vec![
        sample_utxo("box1", 1_000_000_000, vec![]),
        sample_utxo("box2", 1_000_000_000, vec![]),
        sample_utxo("box3", 1_000_000_000, vec![]),
    ];

    // Need 2.5 ERG - should select 3 boxes
    let result = select_erg_inputs(&utxos, 2_500_000_000).unwrap();
    assert_eq!(result.boxes.len(), 3);
    assert_eq!(result.total_erg, 3_000_000_000);
}

#[test]
fn test_select_erg_inputs_insufficient() {
    let utxos = vec![sample_utxo("box1", 1_000_000_000, vec![])];

    // Need 10 ERG but only have 1
    let result = select_erg_inputs(&utxos, 10_000_000_000);
    assert!(result.is_err());

    match result {
        Err(BuildError::InsufficientBalance {
            required,
            available,
        }) => {
            assert_eq!(required, 10_000_000_000);
            assert_eq!(available, 1_000_000_000);
        }
        _ => panic!("Expected InsufficientBalance error"),
    }
}

#[test]
fn test_select_token_inputs_success() {
    let token_id = "abc123".to_string();
    let utxos = vec![
        sample_utxo("box1", 1_000_000_000, vec![(token_id.clone(), 100)]),
        sample_utxo("box2", 2_000_000_000, vec![]),
    ];

    let result = select_token_inputs(&utxos, &token_id, 50, 500_000_000).unwrap();
    assert_eq!(result.boxes.len(), 1);
    assert_eq!(result.token_amount, 100);
    assert_eq!(result.total_erg, 1_000_000_000);
}

#[test]
fn test_select_token_inputs_needs_more_erg() {
    let token_id = "abc123".to_string();
    let utxos = vec![
        sample_utxo("box1", 100_000_000, vec![(token_id.clone(), 100)]), // Low ERG
        sample_utxo("box2", 2_000_000_000, vec![]),                      // No tokens
    ];

    // Need 50 tokens and 1 ERG
    let result = select_token_inputs(&utxos, &token_id, 50, 1_000_000_000).unwrap();
    assert_eq!(result.boxes.len(), 2); // Need both boxes
    assert_eq!(result.token_amount, 100);
    assert_eq!(result.total_erg, 2_100_000_000);
}

#[test]
fn test_select_token_inputs_insufficient_tokens() {
    let token_id = "abc123".to_string();
    let utxos = vec![sample_utxo(
        "box1",
        1_000_000_000,
        vec![(token_id.clone(), 50)],
    )];

    // Need 100 tokens but only have 50
    let result = select_token_inputs(&utxos, &token_id, 100, 500_000_000);
    assert!(result.is_err());

    match result {
        Err(BuildError::InsufficientTokens {
            token,
            required,
            available,
        }) => {
            assert_eq!(token, token_id);
            assert_eq!(required, 100);
            assert_eq!(available, 50);
        }
        _ => panic!("Expected InsufficientTokens error"),
    }
}

#[test]
fn test_constants() {
    assert_eq!(TX_FEE_NANO, 1_000_000);
    assert_eq!(PROXY_EXECUTION_FEE_NANO, 2_000_000);
    assert_eq!(MIN_BOX_VALUE_NANO, 1_000_000);
    assert_eq!(BOT_PROCESSING_OVERHEAD, 3_000_000);
    assert_eq!(REFUND_HEIGHT_OFFSET, 720);
}

#[test]
fn test_user_utxo_struct() {
    let utxo = UserUtxo {
        box_id: "a".repeat(64),
        tx_id: "b".repeat(64),
        index: 0,
        value: 1_000_000_000,
        ergo_tree: "0008cd...".to_string(),
        assets: vec![("token1".to_string(), 100)],
        creation_height: 12345,
        registers: HashMap::new(),
    };

    assert_eq!(utxo.value, 1_000_000_000);
    assert_eq!(utxo.assets.len(), 1);
}

#[test]
fn test_lend_request_struct() {
    let req = LendRequest {
        pool_id: "erg".to_string(),
        amount: 1_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
        min_lp_tokens: Some(100),
        slippage_bps: 0,
    };

    assert_eq!(req.pool_id, "erg");
    assert_eq!(req.amount, 1_000_000_000);
    assert!(req.min_lp_tokens.is_some());
}

#[test]
fn test_withdraw_request_struct() {
    let req = WithdrawRequest {
        pool_id: "sigusd".to_string(),
        lp_amount: 1000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
        min_output: None,
    };

    assert_eq!(req.pool_id, "sigusd");
    assert_eq!(req.lp_amount, 1000);
    assert!(req.min_output.is_none());
}

#[test]
fn test_borrow_request_struct() {
    let req = BorrowRequest {
        pool_id: "sigusd".to_string(),
        collateral_token: "native".to_string(),
        collateral_amount: 10_000_000_000,
        borrow_amount: 100_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
    };

    assert_eq!(req.pool_id, "sigusd");
    assert_eq!(req.collateral_token, "native");
}

#[test]
fn test_repay_request_struct() {
    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: "a".repeat(64),
        repay_amount: 5_000_000_000,
        total_owed: 5_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
    };

    assert_eq!(req.pool_id, "erg");
    assert_eq!(req.collateral_box_id.len(), 64);
}

#[test]
fn test_refund_request_struct() {
    let req = RefundRequest {
        proxy_box_id: "a".repeat(64),
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
    };

    assert_eq!(req.proxy_box_id.len(), 64);
}

#[test]
fn test_tx_summary_struct() {
    let summary = TxSummary {
        action: "lend".to_string(),
        pool_id: "erg".to_string(),
        pool_name: "ERG Pool".to_string(),
        amount_in: "10.0 ERG".to_string(),
        amount_out_estimate: Some("~100 LP".to_string()),
        proxy_address: TEST_ADDRESS.to_string(),
        refund_height: 1000720,
        service_fee_raw: 62500000,
        service_fee_display: "0.062500 ERG".to_string(),
        total_to_send_raw: 10_062_500_000,
        total_to_send_display: "10.062500 ERG".to_string(),
    };

    assert_eq!(summary.action, "lend");
    assert!(summary.amount_out_estimate.is_some());
}

#[test]
fn test_build_response_struct() {
    let response = BuildResponse {
        unsigned_tx: "{}".to_string(),
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "withdraw".to_string(),
            pool_id: "erg".to_string(),
            pool_name: "ERG Pool".to_string(),
            amount_in: "100 LP".to_string(),
            amount_out_estimate: None,
            proxy_address: TEST_ADDRESS.to_string(),
            refund_height: 1000720,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: 0,
            total_to_send_display: String::new(),
        },
    };

    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert!(response.summary.amount_out_estimate.is_none());
}

#[test]
fn test_user_utxo_to_eip12() {
    let utxo = sample_utxo("box123", 1_000_000_000, vec![("token1".to_string(), 100)]);
    let eip12 = user_utxo_to_eip12(&utxo);

    assert_eq!(eip12.box_id, "box123");
    assert_eq!(eip12.value, "1000000000");
    assert_eq!(eip12.assets.len(), 1);
    assert_eq!(eip12.assets[0].token_id, "token1");
    assert_eq!(eip12.assets[0].amount, "100");
}

#[test]
fn test_build_lend_tx_erg_pool() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let amount: u64 = 10_000_000_000;

    let utxos = vec![sample_utxo(
        "a".repeat(64).as_str(),
        15_000_000_000, // 15 ERG - enough for lend + fees + change
        vec![],
    )];

    let req = LendRequest {
        pool_id: "erg".to_string(),
        amount,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: utxos,
        min_lp_tokens: Some(100),
        slippage_bps: 0,
    };

    let result = build_lend_tx(req, config, current_height);
    assert!(result.is_ok(), "build_lend_tx failed: {:?}", result.err());

    let response = result.unwrap();
    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert_eq!(response.summary.action, "lend");
    assert_eq!(response.summary.pool_id, "erg");
    assert_eq!(
        response.summary.refund_height,
        current_height + REFUND_HEIGHT_OFFSET
    );

    // 10 ERG / 160 = 62_500_000 nanoERG (above MIN_BOX_VALUE_NANO minimum)
    assert_eq!(response.summary.service_fee_raw, 62_500_000);
    assert_eq!(response.summary.total_to_send_raw, amount + 62_500_000); // no slippage

    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    assert!(tx["inputs"].is_array());
    assert!(tx["outputs"].is_array());

    assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

    let proxy_value: i64 = tx["outputs"][0]["value"].as_str().unwrap().parse().unwrap();
    let expected_proxy = (amount + 62_500_000) as i64 + BOT_PROCESSING_OVERHEAD;
    assert_eq!(proxy_value, expected_proxy);
}

#[test]
fn test_build_lend_tx_zero_amount() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let req = LendRequest {
        pool_id: "erg".to_string(),
        amount: 0, // Zero amount
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
        min_lp_tokens: None,
        slippage_bps: 0,
    };

    let result = build_lend_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InvalidAmount(msg)) => {
            assert!(msg.contains("greater than 0"));
        }
        _ => panic!("Expected InvalidAmount error"),
    }
}

#[test]
fn test_build_withdraw_tx_success() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let lp_token_id = config.lend_token_id.to_string();
    let lp_amount: u64 = 1000;

    let utxos = vec![sample_utxo(
        "b".repeat(64).as_str(),
        10_000_000_000,                    // 10 ERG
        vec![(lp_token_id.clone(), 5000)], // 5000 LP tokens
    )];

    let req = WithdrawRequest {
        pool_id: "erg".to_string(),
        lp_amount,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: utxos,
        min_output: Some(9_000_000_000), // Expect at least 9 ERG back
    };

    let result = build_withdraw_tx(req, config, current_height);
    assert!(
        result.is_ok(),
        "build_withdraw_tx failed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert_eq!(response.summary.action, "withdraw");
    assert_eq!(response.summary.pool_id, "erg");
    assert_eq!(
        response.summary.refund_height,
        current_height + REFUND_HEIGHT_OFFSET
    );

    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    assert!(tx["inputs"].is_array());
    assert!(tx["outputs"].is_array());

    assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

    let proxy_output = &tx["outputs"][0];
    assert!(!proxy_output["assets"].as_array().unwrap().is_empty());
}

#[test]
fn test_build_withdraw_tx_insufficient_lp() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let lp_token_id = config.lend_token_id.to_string();

    let utxos = vec![sample_utxo(
        "c".repeat(64).as_str(),
        10_000_000_000,                  // 10 ERG
        vec![(lp_token_id.clone(), 50)], // Only 50 LP tokens
    )];

    let req = WithdrawRequest {
        pool_id: "erg".to_string(),
        lp_amount: 1000, // Want 1000 LP
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: utxos,
        min_output: None,
    };

    let result = build_withdraw_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InsufficientTokens {
            token,
            required,
            available,
        }) => {
            assert_eq!(token, lp_token_id);
            assert_eq!(required, 1000);
            assert_eq!(available, 50);
        }
        _ => panic!("Expected InsufficientTokens error"),
    }
}

#[test]
fn test_miner_fee_ergo_tree_constant() {
    assert!(MINER_FEE_ERGO_TREE.starts_with("1005040004000e36"));
}

#[test]
fn test_build_repay_tx_erg_pool() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let repay_amount: u64 = 5_000_000_000;
    let collateral_box_id = "a".repeat(64);

    let utxos = vec![sample_utxo(
        "d".repeat(64).as_str(),
        10_000_000_000, // 10 ERG - enough for repay + fees + change
        vec![],
    )];

    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: collateral_box_id.clone(),
        repay_amount,
        total_owed: repay_amount,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: utxos,
    };

    let result = build_repay_tx(req, config, current_height);
    assert!(result.is_ok(), "build_repay_tx failed: {:?}", result.err());

    let response = result.unwrap();
    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert_eq!(response.summary.action, "repay");
    assert_eq!(response.summary.pool_id, "erg");
    assert_eq!(
        response.summary.refund_height,
        current_height + REFUND_HEIGHT_OFFSET
    );
    assert!(response.summary.amount_out_estimate.is_some());

    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    assert!(tx["inputs"].is_array());
    assert!(tx["outputs"].is_array());

    assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

    let proxy_output = &tx["outputs"][0];
    let registers = &proxy_output["additionalRegisters"];
    assert!(registers["R4"].is_string()); // neededAmount
    assert!(registers["R5"].is_string()); // borrower
    assert!(registers["R6"].is_string()); // refundHeight
    assert!(registers["R7"].is_string()); // collateralBoxId
}

#[test]
fn test_build_repay_tx_zero_amount() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: "a".repeat(64),
        repay_amount: 0, // Zero amount
        total_owed: 1_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![],
    };

    let result = build_repay_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InvalidAmount(msg)) => {
            assert!(msg.contains("greater than 0"));
        }
        _ => panic!("Expected InvalidAmount error"),
    }
}

#[test]
fn test_build_repay_tx_invalid_collateral_id_too_short() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: "abc123".to_string(), // Too short - not 64 chars
        repay_amount: 1_000_000_000,
        total_owed: 1_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo("e".repeat(64).as_str(), 10_000_000_000, vec![])],
    };

    let result = build_repay_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InvalidAmount(msg)) => {
            assert!(msg.contains("64 hex characters"));
        }
        _ => panic!("Expected InvalidAmount error for invalid collateral box ID"),
    }
}

#[test]
fn test_build_repay_tx_invalid_collateral_id_not_hex() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let invalid_hex = "g".repeat(64);

    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: invalid_hex,
        repay_amount: 1_000_000_000,
        total_owed: 1_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo("f".repeat(64).as_str(), 10_000_000_000, vec![])],
    };

    let result = build_repay_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::TxBuildError(msg)) => {
            assert!(msg.contains("hex"));
        }
        _ => panic!("Expected TxBuildError for invalid hex"),
    }
}

#[test]
fn test_build_repay_tx_insufficient_balance() {
    use crate::constants::get_pool;

    let config = get_pool("erg").unwrap();
    let current_height = 1_000_000;

    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: "a".repeat(64),
        repay_amount: 10_000_000_000, // 10 ERG
        total_owed: 10_000_000_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo(
            "g".repeat(64).as_str(),
            1_000_000_000, // Only 1 ERG
            vec![],
        )],
    };

    let result = build_repay_tx(req, config, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InsufficientBalance {
            required,
            available,
        }) => {
            // Required: 10_000_000_000 + 3_000_000 + 1_000_000 + 1_000_000 = 10_005_000_000
            assert!(required > 10_000_000_000);
            assert_eq!(available, 1_000_000_000);
        }
        _ => panic!("Expected InsufficientBalance error"),
    }
}

#[test]
fn test_build_borrow_tx_token_pool_success() {
    use crate::constants::get_pool;
    use crate::state::CollateralOption;

    let config = get_pool("sigusd").unwrap();
    let collateral_config = CollateralOption {
        token_id: "native".to_string(),
        token_name: "ERG".to_string(),
        liquidation_threshold: 1250,
        liquidation_penalty: 500,
        dex_nft: Some(
            "9916d75132593c8b07fe18bd8d583bda1652eed7565cf41a4738ddd90fc992ec".to_string(),
        ),
    };
    let current_height = 1_000_000;

    let req = BorrowRequest {
        pool_id: "sigusd".to_string(),
        collateral_token: "native".to_string(),
        collateral_amount: 10_000_000_000, // 10 ERG as collateral
        borrow_amount: 10_000,             // 100 SigUSD (2 decimals)
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 15_000_000_000, vec![])],
    };

    let result = build_borrow_tx(req, config, &collateral_config, current_height);
    assert!(result.is_ok(), "Expected Ok, got {:?}", result.err());

    let response = result.unwrap();
    assert_eq!(response.summary.action, "borrow");
    assert_eq!(response.summary.pool_id, "sigusd");

    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).expect("Valid JSON");
    let outputs = tx["outputs"].as_array().unwrap();
    assert!(outputs.len() >= 2);

    let proxy_value: i64 = outputs[0]["value"].as_str().unwrap().parse().unwrap();
    assert!(proxy_value > 10_000_000_000);
}

#[test]
fn test_build_borrow_tx_missing_dex_nft() {
    use crate::constants::get_pool;
    use crate::state::CollateralOption;

    let config = get_pool("sigusd").unwrap();
    let collateral_config = CollateralOption {
        token_id: "native".to_string(),
        token_name: "ERG".to_string(),
        liquidation_threshold: 1250,
        liquidation_penalty: 500,
        dex_nft: None, // Missing DEX NFT should cause error
    };
    let current_height = 1_000_000;

    let req = BorrowRequest {
        pool_id: "sigusd".to_string(),
        collateral_token: "native".to_string(),
        collateral_amount: 10_000_000_000,
        borrow_amount: 10_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 15_000_000_000, vec![])],
    };

    let result = build_borrow_tx(req, config, &collateral_config, current_height);
    assert!(result.is_err());
    match result {
        Err(BuildError::TxBuildError(msg)) => {
            assert!(msg.contains("DEX NFT"));
        }
        _ => panic!("Expected TxBuildError about missing DEX NFT"),
    }
}

#[test]
fn test_build_borrow_tx_insufficient_collateral() {
    use crate::constants::get_pool;
    use crate::state::CollateralOption;

    let config = get_pool("sigusd").unwrap();
    let collateral_config = CollateralOption {
        token_id: "native".to_string(),
        token_name: "ERG".to_string(),
        liquidation_threshold: 1250,
        liquidation_penalty: 500,
        dex_nft: Some(
            "9916d75132593c8b07fe18bd8d583bda1652eed7565cf41a4738ddd90fc992ec".to_string(),
        ),
    };
    let current_height = 1_000_000;

    let req = BorrowRequest {
        pool_id: "sigusd".to_string(),
        collateral_token: "native".to_string(),
        collateral_amount: 10_000_000_000,
        borrow_amount: 10_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 1_000_000_000, vec![])],
    };

    let result = build_borrow_tx(req, config, &collateral_config, current_height);
    assert!(matches!(
        result,
        Err(BuildError::InsufficientBalance { .. })
    ));
}

fn sample_proxy_box(
    box_id: &str,
    value: i64,
    assets: Vec<(String, i64)>,
    refund_height: i64,
) -> ProxyBoxData {
    let user_ergo_tree =
        "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f".to_string();

    ProxyBoxData {
            box_id: box_id.to_string(),
            tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                .to_string(),
            index: 0,
            value,
            ergo_tree: "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304".to_string(), // Dummy proxy contract
            assets,
            creation_height: 1000000,
            user_ergo_tree,
            r6_refund_height: refund_height,
            is_repay_proxy: false,
            additional_registers: HashMap::new(),
        }
}

#[test]
fn test_build_refund_tx_success() {
    let current_height = 1_001_000; // Well past refund height
    let refund_height = 1_000_720; // Was set 720 blocks ago

    let proxy_box = sample_proxy_box(
        &"a".repeat(64),
        10_000_000_000, // 10 ERG
        vec![],
        refund_height,
    );

    let result = build_refund_tx(proxy_box.clone(), current_height);
    assert!(result.is_ok(), "build_refund_tx failed: {:?}", result.err());

    let response = result.unwrap();
    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert_eq!(response.refundable_after_height, refund_height);

    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    assert!(tx["inputs"].is_array());
    assert!(tx["outputs"].is_array());

    let outputs = tx["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 2);

    let primary_output = &outputs[0];
    let expected_primary_value = proxy_box.value - TX_FEE_NANO;
    assert_eq!(
        primary_output["value"].as_str().unwrap(),
        expected_primary_value.to_string()
    );

    let r4 = primary_output["additionalRegisters"]["R4"]
        .as_str()
        .unwrap();
    assert!(r4.starts_with("0e20")); // Coll[Byte] prefix for 32 bytes
    assert!(r4.contains(&"a".repeat(64))); // Contains box ID

    let fee_output = &outputs[1];
    assert_eq!(
        fee_output["value"].as_str().unwrap(),
        TX_FEE_NANO.to_string()
    );
    assert_eq!(
        fee_output["ergoTree"].as_str().unwrap(),
        MINER_FEE_ERGO_TREE
    );
}

#[test]
fn test_build_refund_tx_with_tokens() {
    let current_height = 1_001_000;
    let refund_height = 1_000_720;

    let token_id = "b".repeat(64);
    let proxy_box = sample_proxy_box(
        &"a".repeat(64),
        5_000_000_000, // 5 ERG
        vec![(token_id.clone(), 1000)],
        refund_height,
    );

    let result = build_refund_tx(proxy_box, current_height);
    assert!(
        result.is_ok(),
        "build_refund_tx with tokens failed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();

    let refund_output = &tx["outputs"][0];
    let assets = refund_output["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["tokenId"].as_str().unwrap(), token_id);
    assert_eq!(assets[0]["amount"].as_str().unwrap(), "1000");
}

#[test]
fn test_build_refund_tx_before_height_still_works() {
    // proveDlog(userPk) spending path has no height check,
    // so refund should succeed even before refund height
    let current_height = 1_000_000; // Before refund height
    let refund_height = 1_000_720;

    let proxy_box = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], refund_height);

    let result = build_refund_tx(proxy_box, current_height);
    assert!(
        result.is_ok(),
        "Refund should work before height via proveDlog: {:?}",
        result.err()
    );
}

#[test]
fn test_build_refund_tx_exactly_at_refund_height() {
    let current_height = 1_000_720;
    let refund_height = 1_000_720;

    let proxy_box = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], refund_height);

    let result = build_refund_tx(proxy_box, current_height);
    assert!(result.is_ok(), "Refund at exact height should succeed");
}

#[test]
fn test_build_refund_tx_insufficient_value() {
    let current_height = 1_001_000;
    let refund_height = 1_000_720;

    let proxy_box = sample_proxy_box(
        &"a".repeat(64),
        1_500_000, // Not enough for MIN_BOX_VALUE + TX_FEE = 2_000_000
        vec![],
        refund_height,
    );

    let result = build_refund_tx(proxy_box, current_height);
    assert!(result.is_err());

    match result {
        Err(BuildError::InsufficientBalance {
            required,
            available,
        }) => {
            assert_eq!(required, MIN_BOX_VALUE_NANO + TX_FEE_NANO);
            assert_eq!(available, 1_500_000);
        }
        _ => panic!("Expected InsufficientBalance error"),
    }
}

#[test]
fn test_build_refund_tx_minimum_viable_value() {
    let current_height = 1_001_000;
    let refund_height = 1_000_720;

    let min_required = MIN_BOX_VALUE_NANO + TX_FEE_NANO;
    let proxy_box = sample_proxy_box(&"a".repeat(64), min_required, vec![], refund_height);

    let result = build_refund_tx(proxy_box, current_height);
    assert!(result.is_ok(), "Minimum viable value should succeed");

    let response = result.unwrap();
    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    let outputs = tx["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 2);
    // Primary output gets MIN_BOX_VALUE_NANO (2M - 1M fee)
    assert_eq!(
        outputs[0]["value"].as_str().unwrap(),
        MIN_BOX_VALUE_NANO.to_string()
    );
}

#[test]
fn test_build_refund_tx_repay_proxy_three_outputs() {
    let current_height = 1_001_000;
    let refund_height = 1_000_720;

    let mut proxy_box = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], refund_height);
    proxy_box.is_repay_proxy = true;

    let result = build_refund_tx(proxy_box, current_height);
    assert!(
        result.is_ok(),
        "Repay proxy refund should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
    let outputs = tx["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 3);
    assert_eq!(
        outputs[1]["value"].as_str().unwrap(),
        MIN_BOX_VALUE_NANO.to_string()
    );
}

#[test]
fn test_proxy_box_data_struct() {
    let proxy_box = ProxyBoxData {
        box_id: "a".repeat(64),
        tx_id: "b".repeat(64),
        index: 0,
        value: 10_000_000_000,
        ergo_tree: "0008cd...".to_string(),
        assets: vec![("token1".to_string(), 100)],
        creation_height: 1000000,
        user_ergo_tree: "0008cd...".to_string(),
        r6_refund_height: 1000720,
        is_repay_proxy: false,
        additional_registers: HashMap::new(),
    };

    assert_eq!(proxy_box.value, 10_000_000_000);
    assert_eq!(proxy_box.assets.len(), 1);
    assert_eq!(proxy_box.r6_refund_height, 1000720);
}

#[test]
fn test_refund_response_struct() {
    let response = RefundResponse {
        unsigned_tx: "{}".to_string(),
        fee_nano: TX_FEE_NANO,
        refundable_after_height: 1000720,
    };

    assert_eq!(response.fee_nano, TX_FEE_NANO);
    assert_eq!(response.refundable_after_height, 1000720);
}
