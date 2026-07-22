use super::*;

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
