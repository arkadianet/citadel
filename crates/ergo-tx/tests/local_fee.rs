use ergo_tx::{build_send_tx, resolved_dev_fee_config, Eip12InputBox, DEFAULT_DEV_FEE_ERGO_TREE};

// Integration test deliberately uses the production resolver (no cfg(test)
// behavior or with_test_dev_fee override in the library).
#[test]
fn local_wallet_send_has_only_the_miner_fee() {
    let input = Eip12InputBox {
        box_id: "aa".repeat(32),
        transaction_id: "bb".repeat(32),
        index: 0,
        value: "5000000".into(),
        ergo_tree: "0008cdowner".into(),
        assets: vec![],
        creation_height: 1,
        additional_registers: Default::default(),
        extension: Default::default(),
    };
    assert_eq!(resolved_dev_fee_config().budget(), 0);
    assert!(!ergo_tx::dev_fee::try_load_from_env().unwrap().enabled);
    let result = build_send_tx(
        &[input],
        "0008cdrecipient",
        "0008cdowner",
        2_000_000,
        None,
        100,
    )
    .unwrap();
    assert_eq!(result.summary.citadel_fee_nano, 0);
    assert_eq!(result.summary.miner_fee, 1_100_000);
    assert_eq!(result.summary.change_erg, 1_900_000);
    assert_eq!(result.unsigned_tx.outputs.len(), 3);
    assert!(result
        .unsigned_tx
        .outputs
        .iter()
        .all(|b| b.ergo_tree != DEFAULT_DEV_FEE_ERGO_TREE));
}
