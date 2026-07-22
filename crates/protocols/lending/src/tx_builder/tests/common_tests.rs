use super::*;
use super::super::common::user_utxo_to_eip12;

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
fn test_miner_fee_ergo_tree_constant() {
    assert!(MINER_FEE_ERGO_TREE.starts_with("1005040004000e36"));
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
