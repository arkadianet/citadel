use super::*;

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
