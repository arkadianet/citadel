use std::collections::HashMap;

use ergo_tx::{with_test_dev_fee, DevFeeConfig};

fn no_citadel_fee<R>(f: impl FnOnce() -> R) -> R {
    with_test_dev_fee(DevFeeConfig::disabled(), f)
}

use super::*;
use crate::calculator;
use crate::state::{AmmPool, PoolType, SwapInput, TokenAmount};
use ergo_tx::Eip12Asset;

fn test_n2t_pool() -> AmmPool {
    AmmPool {
        pool_id: "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        pool_type: PoolType::N2T,
        box_id: "pool_box_1".to_string(),
        erg_reserves: Some(100_000_000_000), // 100 ERG
        token_x: None,
        token_y: TokenAmount {
            token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: 1_000_000,
            decimals: Some(6),
            name: Some("TestToken".to_string()),
        },
        lp_token_id: "lp_token".to_string(),
        lp_circulating: 1000,
        fee_num: 997,
        fee_denom: 1000,
    }
}

fn test_pool_box() -> Eip12InputBox {
    Eip12InputBox {
        box_id: "pool_box_1".to_string(),
        transaction_id: "pool_tx_1".to_string(),
        index: 0,
        value: "100000000000".to_string(), // 100 ERG
        ergo_tree: "pool_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset {
                token_id: "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "1".to_string(),
            },
            Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "9223372036854774807".to_string(), // max LP supply minus circulating
            },
            Eip12Asset {
                token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "1000000".to_string(),
            },
        ],
        creation_height: 999_000,
        additional_registers: {
            let mut m = HashMap::new();
            m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997 (sigma Int)
            m
        },
        extension: HashMap::new(),
    }
}

fn test_user_utxo() -> Eip12InputBox {
    Eip12InputBox {
        box_id: "user_utxo_1".to_string(),
        transaction_id: "user_tx_1".to_string(),
        index: 0,
        value: "10000000000".to_string(), // 10 ERG
        ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
            .to_string(),
        assets: vec![],
        creation_height: 999_000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}

#[test]
fn test_direct_swap_erg_to_token() {
    no_citadel_fee(|| {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo();

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        }; // 1 ERG
        let output =
            calculator::calculate_output(100_000_000_000, 1_000_000, 1_000_000_000, 997, 1000);
        let min_output = calculator::apply_slippage(output, 0.5);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            min_output,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        assert_eq!(build.unsigned_tx.outputs[0].ergo_tree, "pool_ergo_tree_hex");
        let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 + 1_000_000_000);
        let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
            .amount
            .parse()
            .unwrap();
        assert_eq!(new_token_y, 1_000_000 - output);

        assert_eq!(build.unsigned_tx.outputs[1].assets.len(), 1);
        let user_token_received: u64 = build.unsigned_tx.outputs[1].assets[0]
            .amount
            .parse()
            .unwrap();
        assert_eq!(user_token_received, output);
        let user_out_erg: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert!(
            user_out_erg > MIN_BOX_VALUE,
            "Change ERG should be folded in"
        );

        assert_eq!(build.unsigned_tx.outputs[2].value, TX_FEE.to_string());

        assert_eq!(build.summary.input_amount, 1_000_000_000);
        assert_eq!(build.summary.input_token, "ERG");
        assert_eq!(build.summary.output_amount, output);
        assert_eq!(build.summary.output_token, "TestToken");
        assert_eq!(build.summary.miner_fee, TX_FEE);
    });
}

#[test]
fn test_direct_swap_token_to_erg() {
    let pool = test_n2t_pool();
    let pool_box = test_pool_box();
    let token_id =
        "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    let user_utxo = Eip12InputBox {
        assets: vec![Eip12Asset {
            token_id: token_id.clone(),
            amount: "50000".to_string(),
        }],
        ..test_user_utxo()
    };

    let input = SwapInput::Token {
        token_id: token_id.clone(),
        amount: 10000,
    };
    let output = calculator::calculate_output(1_000_000, 100_000_000_000, 10000, 997, 1000);
    let min_output = calculator::apply_slippage(output, 0.5);

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        min_output,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_ok(), "Should build: {:?}", result.err());
    let build = result.unwrap();

    assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");

    let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
    assert_eq!(new_pool_erg, 100_000_000_000 - output);
    let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
        .amount
        .parse()
        .unwrap();
    assert_eq!(new_token_y, 1_000_000 + 10000);

    let user_out = &build.unsigned_tx.outputs[1];
    let user_erg_received: u64 = user_out.value.parse().unwrap();
    assert!(user_erg_received > output, "Change ERG should be folded in");

    let change_token: &Eip12Asset = user_out
        .assets
        .iter()
        .find(|a| a.token_id == token_id)
        .unwrap();
    assert_eq!(change_token.amount, "40000"); // 50000 - 10000
}

#[test]
fn test_direct_swap_insufficient_erg() {
    let pool = test_n2t_pool();
    let pool_box = test_pool_box();
    let user_utxo = Eip12InputBox {
        value: "1000000".to_string(), // 0.001 ERG - not enough
        ..test_user_utxo()
    };

    let input = SwapInput::Erg {
        amount: 1_000_000_000,
    };
    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Insufficient"));
}

#[test]
fn test_direct_swap_slippage_exceeded() {
    let pool = test_n2t_pool();
    let pool_box = test_pool_box();
    let user_utxo = test_user_utxo();

    let input = SwapInput::Erg {
        amount: 1_000_000_000,
    };
    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        u64::MAX,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("below minimum") || err.contains("Output below minimum"),
        "Got: {}",
        err
    );
}

#[test]
fn test_direct_swap_pool_registers_preserved() {
    let pool = test_n2t_pool();
    let pool_box = test_pool_box();
    let user_utxo = test_user_utxo();

    let input = SwapInput::Erg {
        amount: 1_000_000_000,
    };
    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    )
    .unwrap();

    assert_eq!(
        result.unsigned_tx.outputs[0].additional_registers.get("R4"),
        Some(&"04ca0f".to_string())
    );
}

fn test_t2t_pool() -> AmmPool {
    AmmPool {
        pool_id: "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        pool_type: PoolType::T2T,
        box_id: "t2t_pool_box_1".to_string(),
        erg_reserves: None,
        token_x: Some(TokenAmount {
            token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: 10_000_000,
            decimals: Some(6),
            name: Some("TokenX".to_string()),
        }),
        token_y: TokenAmount {
            token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: 5_000_000,
            decimals: Some(6),
            name: Some("TokenY".to_string()),
        },
        lp_token_id: "t2t_lp_token".to_string(),
        lp_circulating: 1000,
        fee_num: 997,
        fee_denom: 1000,
    }
}

fn test_t2t_pool_box() -> Eip12InputBox {
    Eip12InputBox {
        box_id: "t2t_pool_box_1".to_string(),
        transaction_id: "t2t_pool_tx_1".to_string(),
        index: 0,
        value: "10000000".to_string(), // 0.01 ERG (storage rent only)
        ergo_tree: "t2t_pool_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset {
                token_id:
                    "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                amount: "1".to_string(),
            },
            Eip12Asset {
                token_id: "t2t_lp_token".to_string(),
                amount: "9223372036854774807".to_string(),
            },
            Eip12Asset {
                token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "10000000".to_string(),
            },
            Eip12Asset {
                token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "5000000".to_string(),
            },
        ],
        creation_height: 999_000,
        additional_registers: {
            let mut m = HashMap::new();
            m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997
            m
        },
        extension: HashMap::new(),
    }
}

fn test_user_utxo_with_token_x() -> Eip12InputBox {
    Eip12InputBox {
        box_id: "user_utxo_token_x".to_string(),
        transaction_id: "user_tx_2".to_string(),
        index: 0,
        value: "5000000000".to_string(), // 5 ERG
        ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
            .to_string(),
        assets: vec![Eip12Asset {
            token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: "500000".to_string(),
        }],
        creation_height: 999_000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}

fn test_user_utxo_with_token_y() -> Eip12InputBox {
    Eip12InputBox {
        box_id: "user_utxo_token_y".to_string(),
        transaction_id: "user_tx_3".to_string(),
        index: 0,
        value: "5000000000".to_string(), // 5 ERG
        ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
            .to_string(),
        assets: vec![Eip12Asset {
            token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: "500000".to_string(),
        }],
        creation_height: 999_000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}

#[test]
fn test_direct_swap_t2t_x_to_y() {
    let pool = test_t2t_pool();
    let pool_box = test_t2t_pool_box();
    let user_utxo = test_user_utxo_with_token_x();

    let input_amount = 100_000u64;
    let input = SwapInput::Token {
        token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        amount: input_amount,
    };
    let output = calculator::calculate_output(10_000_000, 5_000_000, input_amount, 997, 1000);
    let min_output = calculator::apply_slippage(output, 0.5);

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        min_output,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(
        result.is_ok(),
        "Should build T2T X->Y swap: {:?}",
        result.err()
    );
    let build = result.unwrap();

    assert_eq!(build.unsigned_tx.inputs[0].box_id, "t2t_pool_box_1");

    let new_pool = &build.unsigned_tx.outputs[0];
    assert_eq!(new_pool.ergo_tree, "t2t_pool_ergo_tree_hex");
    assert_eq!(new_pool.value, "10000000");
    assert_eq!(new_pool.assets.len(), 4);

    let new_x: u64 = new_pool.assets[2].amount.parse().unwrap();
    assert_eq!(new_x, 10_000_000 + input_amount);

    let new_y: u64 = new_pool.assets[3].amount.parse().unwrap();
    assert_eq!(new_y, 5_000_000 - output);

    let user_out = &build.unsigned_tx.outputs[1];
    let received_token = user_out
        .assets
        .iter()
        .find(|a| {
            a.token_id == "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        })
        .expect("User should receive token_y");
    assert_eq!(received_token.amount, output.to_string());

    assert_eq!(build.summary.input_amount, input_amount);
    assert_eq!(build.summary.input_token, "TokenX");
    assert_eq!(build.summary.output_amount, output);
    assert_eq!(build.summary.output_token, "TokenY");
}

#[test]
fn test_direct_swap_t2t_y_to_x() {
    let pool = test_t2t_pool();
    let pool_box = test_t2t_pool_box();
    let user_utxo = test_user_utxo_with_token_y();

    let input_amount = 100_000u64;
    let input = SwapInput::Token {
        token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        amount: input_amount,
    };
    let output = calculator::calculate_output(5_000_000, 10_000_000, input_amount, 997, 1000);
    let min_output = calculator::apply_slippage(output, 0.5);

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        min_output,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(
        result.is_ok(),
        "Should build T2T Y->X swap: {:?}",
        result.err()
    );
    let build = result.unwrap();

    assert_eq!(build.unsigned_tx.outputs[0].value, "10000000");
    assert_eq!(build.unsigned_tx.outputs[0].assets.len(), 4);

    let new_x: u64 = build.unsigned_tx.outputs[0].assets[2]
        .amount
        .parse()
        .unwrap();
    assert_eq!(new_x, 10_000_000 - output);

    let new_y: u64 = build.unsigned_tx.outputs[0].assets[3]
        .amount
        .parse()
        .unwrap();
    assert_eq!(new_y, 5_000_000 + input_amount);

    let user_out = &build.unsigned_tx.outputs[1];
    let received_token = user_out
        .assets
        .iter()
        .find(|a| {
            a.token_id == "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        })
        .expect("User should receive token_x");
    assert_eq!(received_token.amount, output.to_string());

    assert_eq!(build.summary.input_token, "TokenY");
    assert_eq!(build.summary.output_token, "TokenX");
}

#[test]
fn test_direct_swap_t2t_pool_erg_unchanged() {
    let pool = test_t2t_pool();
    let pool_box = test_t2t_pool_box();
    let user_utxo = test_user_utxo_with_token_x();

    let input = SwapInput::Token {
        token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        amount: 50_000,
    };

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1, // any output is fine
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    )
    .unwrap();

    let pool_erg_in: u64 = pool_box.value.parse().unwrap();
    let pool_erg_out: u64 = result.unsigned_tx.outputs[0].value.parse().unwrap();
    assert_eq!(pool_erg_in, pool_erg_out, "T2T pool ERG must be unchanged");

    assert_eq!(
        result.unsigned_tx.outputs[0].additional_registers.get("R4"),
        Some(&"04ca0f".to_string()),
        "R4 fee register must be preserved"
    );

    assert_eq!(
        result.unsigned_tx.outputs[0].ergo_tree, pool_box.ergo_tree,
        "Pool ErgoTree must be preserved"
    );
    assert_eq!(
        result.unsigned_tx.outputs[0].assets[0].amount, pool_box.assets[0].amount,
        "NFT amount must be unchanged"
    );
    assert_eq!(
        result.unsigned_tx.outputs[0].assets[1].amount, pool_box.assets[1].amount,
        "LP amount must be unchanged"
    );
}

#[test]
fn test_direct_swap_t2t_wrong_token() {
    let pool = test_t2t_pool();
    let pool_box = test_t2t_pool_box();
    let user_utxo = Eip12InputBox {
        assets: vec![Eip12Asset {
            token_id: "wrong_token_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: "500000".to_string(),
        }],
        ..test_user_utxo_with_token_x()
    };

    let input = SwapInput::Token {
        token_id: "wrong_token_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        amount: 100_000,
    };

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("does not match") || err.contains("Invalid token"),
        "Expected token mismatch error, got: {}",
        err
    );
}

#[test]
fn test_direct_swap_t2t_erg_input_rejected() {
    let pool = test_t2t_pool();
    let pool_box = test_t2t_pool_box();
    let user_utxo = test_user_utxo(); // plain ERG utxo

    let input = SwapInput::Erg {
        amount: 1_000_000_000,
    };

    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("ERG input") || err.contains("not valid for T2T"),
        "Expected ERG rejection error, got: {}",
        err
    );
}

#[test]
fn test_direct_swap_token_to_erg_small_change_folded_into_output() {
    no_citadel_fee(|| {
        // Regression: when user's ERG input minus TX_FEE is below MIN_CHANGE_VALUE
        // and there are no change tokens, the leftover ERG must be folded into the
        // user swap output instead of being silently dropped.
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let token_id =
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

        let user_utxo = Eip12InputBox {
            value: "1956185".to_string(),
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "2192".to_string(),
            }],
            ..test_user_utxo()
        };

        let input = SwapInput::Token {
            token_id: token_id.clone(),
            amount: 2192,
        };
        let output = calculator::calculate_output(1_000_000, 100_000_000_000, 2192, 997, 1000);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let total_input_erg: u64 = build
            .unsigned_tx
            .inputs
            .iter()
            .map(|i| i.value.parse::<u64>().unwrap())
            .sum();
        let total_output_erg: u64 = build
            .unsigned_tx
            .outputs
            .iter()
            .map(|o| o.value.parse::<u64>().unwrap())
            .sum();
        assert_eq!(
            total_input_erg, total_output_erg,
            "ERG inputs ({}) must equal outputs ({})",
            total_input_erg, total_output_erg
        );

        let change_erg = 1_956_185u64 - TX_FEE;
        let user_swap_value: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert_eq!(user_swap_value, output + change_erg);
    });
}

#[test]
fn test_direct_swap_token_to_erg_rejects_pool_dust_breach() {
    let mut pool = test_n2t_pool();
    pool.erg_reserves = Some(10_000_000); // 0.01 ERG — like a drained eTOSI pool
    pool.token_y.amount = 1_000;

    let mut pool_box = test_pool_box();
    pool_box.value = "10000000".to_string();
    pool_box.assets[2].amount = "1000".to_string();

    let token_id = pool.token_y.token_id.clone();
    let user_utxo = Eip12InputBox {
        assets: vec![Eip12Asset {
            token_id: token_id.clone(),
            amount: "20000".to_string(),
        }],
        ..test_user_utxo()
    };

    let input = SwapInput::Token {
        token_id,
        amount: 20_000, // drains past min-box reservation
    };
    let result = build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        1,
        &[user_utxo],
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        1_000_000,
        None,
        None,
    );

    assert!(result.is_err(), "Must reject dust-breaching swap");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("minimum ERG") || err.contains("max extractable"),
        "unexpected error: {}",
        err
    );
}
