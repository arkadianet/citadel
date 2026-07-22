use super::*;

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
