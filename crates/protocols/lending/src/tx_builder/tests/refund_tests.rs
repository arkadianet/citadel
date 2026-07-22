use std::collections::HashMap;

use super::*;

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
