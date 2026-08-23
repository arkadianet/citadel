use super::*;

#[test]
fn test_validate_mint_positive_amount() {
    let state = create_test_state(10000, true);

    assert!(validate_mint_dexy(100, &state).is_ok());

    let result = validate_mint_dexy(0, &state);
    assert!(result.is_err());
    assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));

    let result = validate_mint_dexy(-100, &state);
    assert!(result.is_err());
}

#[test]
fn test_validate_mint_can_mint() {
    let state = create_test_state(0, false);

    let result = validate_mint_dexy(100, &state);
    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(ProtocolError::ActionNotAllowed { .. })
    ));
}

#[test]
fn test_validate_mint_exceeds_available() {
    let state = create_test_state(1000, true);

    let result = validate_mint_dexy(2000, &state);
    assert!(result.is_err());
    assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));
}

#[test]
fn test_calculate_mint_amounts_use() {
    // Test with USE (oracle divisor = 1000)
    // Oracle rate: 1_850_000_000 nanoERG per USD (1.85 ERG per USD)
    // Amount: 1_000 (1 USE with 3 decimals)
    let (bank_erg_added, buyback_fee) =
        calculate_mint_amounts(1_000, 1_850_000_000, DexyVariant::Usd);

    // Adjusted rate = 1_850_000_000 / 1000 = 1_850_000
    // Contract formula (order matters!):
    //   bankRate = 1_850_000 * 1003 / 1000 = 1_855_550
    //   bank_erg_added = 1_000 * 1_855_550 = 1_855_550_000
    //   buybackRate = 1_850_000 * 2 / 1000 = 3_700
    //   buyback_fee = 1_000 * 3_700 = 3_700_000
    assert_eq!(bank_erg_added, 1_855_550_000);
    assert_eq!(buyback_fee, 3_700_000);
}

#[test]
fn test_calculate_mint_amounts_gold() {
    // Test with DexyGold (oracle divisor = 1_000_000)
    // Oracle rate: 220_000_000_000 nanoERG per kg (220 ERG per kg)
    // Amount: 10 (10 DexyGold tokens = 10 mg)
    let (bank_erg_added, buyback_fee) =
        calculate_mint_amounts(10, 220_000_000_000, DexyVariant::Gold);

    // Adjusted rate = 220_000_000_000 / 1_000_000 = 220_000 nanoERG per mg
    // Contract formula (order matters!):
    //   bankRate = 220_000 * 1003 / 1000 = 220_660
    //   bank_erg_added = 10 * 220_660 = 2_206_600
    //   buybackRate = 220_000 * 2 / 1000 = 440
    //   buyback_fee = 10 * 440 = 4_400
    assert_eq!(bank_erg_added, 2_206_600);
    assert_eq!(buyback_fee, 4_400);
}

#[test]
fn test_integer_division_order_matters() {
    // This test verifies our calculation matches contract's integer division exactly
    //
    // The order of operations matters due to integer division:
    // - (amount * rate * 1003) / 1000 gives different result than
    // - amount * (rate * 1003 / 1000)
    //
    // The contract uses the latter form, so we must match it.

    let amount: i64 = 100;
    let oracle_rate_nano: i64 = 2_319_455_000; // Example oracle value
    let variant = DexyVariant::Usd;

    let oracle_rate = oracle_rate_nano / variant.oracle_divisor(); // 2_319_455

    // Wrong order: (amount * rate * 1003) / 1000
    // = (100 * 2_319_455 * 1003) / 1000 = 232_641_336_500 / 1000 = 232_641_336
    let wrong_order = amount * oracle_rate * 1003 / 1000;

    // Contract's order: amount * (rate * 1003 / 1000)
    // = 100 * (2_319_455 * 1003 / 1000) = 100 * 2_326_413 = 232_641_300
    let bank_rate = oracle_rate * 1003 / 1000;
    let contract_order = amount * bank_rate;

    // Our new (correct) calculation
    let (new_bank_erg_added, _) = calculate_mint_amounts(amount, oracle_rate_nano, variant);

    // The two orders give different results (36 nanoERG difference in this case)
    assert_ne!(
        wrong_order, contract_order,
        "Different division order should give different results"
    );

    // New calculation matches contract order exactly
    assert_eq!(
        new_bank_erg_added, contract_order,
        "New calculation {} should equal contract order {}",
        new_bank_erg_added, contract_order
    );
}

#[test]
fn test_tx_summary() {
    let summary = TxSummary {
        action: "mint_dexy_gold".to_string(),
        erg_amount_nano: 1_000_000_000,
        token_amount: 100,
        token_name: "DexyGold".to_string(),
        tx_fee_nano: 1_100_000,
        citadel_fee_nano: 0,
        bank_fee_nano: 3_000_000,
        buyback_fee_nano: 2_000_000,
    };

    assert_eq!(summary.action, "mint_dexy_gold");
    assert_eq!(summary.token_name, "DexyGold");
}

#[test]
fn mint_sends_exactly_the_requested_amount_to_the_recipient() {
    let ctx = create_mint_context(1_000_000, 100_000);
    let state = create_test_state(1_000_000, true);

    let request = MintDexyRequest {
        variant: DexyVariant::Gold,
        amount: 1_000,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(100_000_000_000, vec![])],
        current_height: 100_000,
        recipient_ergo_tree: Some("recipient_ergo_tree".to_string()),
        recipient_held_tokens: 0,
    };

    let build = no_citadel_fee(|| build_mint_dexy_tx(&request, &ctx, &state))
        .expect("mint should build");

    // outputs: [free_mint, bank, buyback, recipient, change?, fee]
    let recipient = &build.unsigned_tx.outputs[3];
    assert_eq!(recipient.ergo_tree, "recipient_ergo_tree");
    assert_eq!(recipient.assets[0].amount, "1000");
}

#[test]
fn mint_tops_up_held_tokens_into_the_recipient_output() {
    let ctx = create_mint_context(1_000_000, 100_000);
    let state = create_test_state(1_000_000, true);
    let token_id = state.dexy_token_id.clone();

    // Wallet holds 266; deliver 1000 by minting only the 734 shortfall.
    let request = MintDexyRequest {
        variant: DexyVariant::Gold,
        amount: 734,
        user_address: "user_addr".to_string(),
        user_ergo_tree: "user_ergo_tree".to_string(),
        user_inputs: vec![create_test_input(
            100_000_000_000,
            vec![(token_id.as_str(), 266)],
        )],
        current_height: 100_000,
        recipient_ergo_tree: Some("recipient_ergo_tree".to_string()),
        recipient_held_tokens: 266,
    };

    let build = no_citadel_fee(|| build_mint_dexy_tx(&request, &ctx, &state))
        .expect("top-up mint should build");

    let recipient = &build.unsigned_tx.outputs[3];
    assert_eq!(recipient.ergo_tree, "recipient_ergo_tree");
    assert_eq!(
        recipient.assets[0].amount, "1000",
        "recipient must receive minted 734 plus held 266"
    );

    // The held tokens moved to the recipient, so none come back as change.
    let change_dexy: i64 = build
        .unsigned_tx
        .outputs
        .iter()
        .skip(4)
        .flat_map(|o| o.assets.iter())
        .filter(|a| a.token_id == token_id)
        .map(|a| a.amount.parse::<i64>().unwrap())
        .sum();
    assert_eq!(change_dexy, 0, "held tokens must not also return as change");
}
