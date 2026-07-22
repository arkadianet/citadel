use super::*;

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
