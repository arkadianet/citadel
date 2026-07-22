//! Golden vectors for dexy `build_*_tx` (Wave 3 Task 17).
//! Lock pre-split behavior; do not edit fixtures after split unless bugfix.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use dexy::constants::DexyVariant;
use dexy::fetch::{DexyLpTxContext, DexySwapTxContext, DexyTxContext};
use dexy::state::DexyState;
use dexy::tx_builder::{
    build_lp_deposit_tx, build_lp_redeem_tx, build_mint_dexy_tx, build_swap_dexy_tx,
    LpDepositRequest, LpRedeemRequest, MintDexyRequest, SwapDexyRequest, SwapDirection,
};
use ergo_lib::ergotree_ir::chain::ergo_box::{
    box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
};
use ergo_lib::ergotree_ir::chain::tx_id::TxId;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_tx::{
    with_test_dev_fee, DevFeeConfig, Eip12Asset, Eip12DataInputBox, Eip12InputBox, Eip12UnsignedTx,
};
use serde_json::Value;

const DEXY_TOKEN_ID: &str = "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
const LP_TOKEN_ID: &str = "cf74432b2d3ab8a1a934b6326a1004e1a19aec7b357c57209018c4aa35226246";
const LP_MINT_NFT_ID: &str = "19b8281b141d19c5b3843a4a77e616d6df05f601e5908159b1eaf3d9da20e664";
const LP_REDEEM_NFT_ID: &str = "08c47eef5c782f146cae5e8cfb5e9d26b18442f82f3c5808b1563b6e3b23f729";
const ORACLE_NFT_ID: &str = "3c45f29a5165b030fdb5eaf5d81f8108f9d8f507b31487dd51f4ae08fe07cf4a";
const SWAP_NFT_ID: &str = "ff7b7eff3c818f9dc573ca03a723a7f6ed1615bf27980ebd4a6c91986b26f801";
const BANK_NFT_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FREE_MINT_NFT_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BUYBACK_NFT_ID: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const INITIAL_LP: i64 = 100_000_000_000;
const HEIGHT: i32 = 100_000;

fn no_citadel_fee<R>(f: impl FnOnce() -> R) -> R {
    with_test_dev_fee(DevFeeConfig::disabled(), f)
}

fn dummy_ergo_box() -> ErgoBox {
    let ergo_tree_bytes = base16::decode(
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    )
    .unwrap();
    let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).unwrap();
    ErgoBox::new(
        BoxValue::new(1_000_000).unwrap(),
        ergo_tree,
        None,
        NonMandatoryRegisters::empty(),
        100000,
        TxId::zero(),
        0,
    )
    .unwrap()
}

fn user_input(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
    Eip12InputBox {
        box_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        transaction_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        index: 0,
        value: value.to_string(),
        ergo_tree: "user_ergo_tree".to_string(),
        assets: tokens
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt))
            .collect(),
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}

fn mint_state() -> DexyState {
    DexyState {
        variant: DexyVariant::Gold,
        bank_erg_nano: 1_000_000_000_000,
        dexy_in_bank: 10_000,
        bank_box_id: "bank_box_123".to_string(),
        dexy_token_id: DEXY_TOKEN_ID.to_string(),
        free_mint_available: 5_000,
        free_mint_reset_height: 1_000_000,
        current_height: HEIGHT,
        oracle_rate_nano: 220_000_000_000,
        oracle_box_id: "oracle_box_456".to_string(),
        lp_erg_reserves: 500_000_000_000,
        lp_dexy_reserves: 500_000,
        lp_box_id: "lp_box_789".to_string(),
        lp_rate_nano: 1_000_000,
        lp_token_reserves: 0,
        lp_circulating: 0,
        can_redeem_lp: true,
        can_mint: true,
        rate_difference_pct: 0.0,
        dexy_circulating: 0,
    }
}

fn mint_context() -> DexyTxContext {
    let buyback_box_id = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let free_mint_input = Eip12InputBox {
        box_id: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string(),
        transaction_id: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
            .to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "free_mint_ergo_tree".to_string(),
        assets: vec![Eip12Asset::new(FREE_MINT_NFT_ID, 1)],
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };
    let bank_input = Eip12InputBox {
        box_id: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string(),
        transaction_id: "1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        index: 0,
        value: "1000000000000".to_string(),
        ergo_tree: "bank_ergo_tree".to_string(),
        assets: vec![
            Eip12Asset::new(BANK_NFT_ID, 1),
            Eip12Asset::new(DEXY_TOKEN_ID, 10_000),
        ],
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };
    let buyback_input = Eip12InputBox {
        box_id: buyback_box_id.to_string(),
        transaction_id: "2222222222222222222222222222222222222222222222222222222222222222"
            .to_string(),
        index: 0,
        value: "5000000".to_string(),
        ergo_tree: "buyback_ergo_tree".to_string(),
        assets: vec![Eip12Asset::new(BUYBACK_NFT_ID, 1)],
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };
    let oracle_data_input = Eip12DataInputBox {
        box_id: "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
        transaction_id: "4444444444444444444444444444444444444444444444444444444444444444"
            .to_string(),
        index: 0,
        value: "1000000".to_string(),
        ergo_tree: "oracle_ergo_tree".to_string(),
        assets: vec![Eip12Asset::new(ORACLE_NFT_ID, 1)],
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
    };
    let lp_data_input = Eip12DataInputBox {
        box_id: "5555555555555555555555555555555555555555555555555555555555555555".to_string(),
        transaction_id: "6666666666666666666666666666666666666666666666666666666666666666"
            .to_string(),
        index: 0,
        value: "500000000000".to_string(),
        ergo_tree: "lp_ergo_tree".to_string(),
        assets: vec![Eip12Asset::new(LP_NFT_ID, 1)],
        creation_height: HEIGHT,
        additional_registers: HashMap::new(),
    };
    let dummy = dummy_ergo_box();
    DexyTxContext {
        free_mint_input,
        free_mint_erg_nano: 1_000_000,
        free_mint_ergo_tree: "free_mint_ergo_tree".to_string(),
        free_mint_r4_height: 200_000,
        free_mint_r5_available: 5_000,
        free_mint_box: dummy.clone(),
        bank_input,
        bank_erg_nano: 1_000_000_000_000,
        dexy_in_bank: 10_000,
        bank_ergo_tree: "bank_ergo_tree".to_string(),
        bank_box: dummy.clone(),
        buyback_input,
        buyback_erg_nano: 5_000_000,
        buyback_ergo_tree: "buyback_ergo_tree".to_string(),
        buyback_box: dummy.clone(),
        oracle_data_input,
        oracle_rate_nano: 220_000_000_000,
        oracle_box: dummy.clone(),
        lp_data_input,
        lp_erg_reserves: 500_000_000_000,
        lp_dexy_reserves: 500_000,
        lp_box: dummy,
    }
}

fn swap_context(lp_erg: i64, lp_dexy: i64) -> DexySwapTxContext {
    let dummy = dummy_ergo_box();
    DexySwapTxContext {
        lp_input: Eip12InputBox {
            box_id: "lp_box_id".to_string(),
            transaction_id: "lp_tx_id".to_string(),
            index: 0,
            value: lp_erg.to_string(),
            ergo_tree: "lp_ergo_tree_hex".to_string(),
            assets: vec![
                Eip12Asset::new(LP_NFT_ID, 1),
                Eip12Asset::new(LP_TOKEN_ID, 9_000_000_000_000_000i64),
                Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
            ],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), 9_000_000_000_000_000),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        swap_input: Eip12InputBox {
            box_id: "swap_box_id".to_string(),
            transaction_id: "swap_tx_id".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "swap_ergo_tree_hex".to_string(),
            assets: vec![Eip12Asset::new(SWAP_NFT_ID, 1)],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        swap_erg_value: 1_000_000,
        swap_ergo_tree: "swap_ergo_tree_hex".to_string(),
        swap_box: dummy,
        swap_tokens: vec![(SWAP_NFT_ID.to_string(), 1)],
    }
}

fn deposit_context(lp_erg: i64, lp_dexy: i64, lp_token_reserves: i64) -> DexyLpTxContext {
    let dummy = dummy_ergo_box();
    DexyLpTxContext {
        lp_input: Eip12InputBox {
            box_id: "lp_box_id".to_string(),
            transaction_id: "lp_tx_id".to_string(),
            index: 0,
            value: lp_erg.to_string(),
            ergo_tree: "lp_ergo_tree_hex".to_string(),
            assets: vec![
                Eip12Asset::new(LP_NFT_ID, 1),
                Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
                Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
            ],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_token_reserves,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        action_input: Eip12InputBox {
            box_id: "mint_box_id".to_string(),
            transaction_id: "mint_tx_id".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "mint_ergo_tree_hex".to_string(),
            assets: vec![Eip12Asset::new(LP_MINT_NFT_ID, 1)],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        action_erg_value: 1_000_000,
        action_ergo_tree: "mint_ergo_tree_hex".to_string(),
        action_box: dummy,
        action_tokens: vec![(LP_MINT_NFT_ID.to_string(), 1)],
        oracle_data_input: None,
        oracle_rate_nano: None,
    }
}

fn redeem_context(
    lp_erg: i64,
    lp_dexy: i64,
    lp_token_reserves: i64,
    oracle_rate_nano: i64,
) -> DexyLpTxContext {
    let dummy = dummy_ergo_box();
    DexyLpTxContext {
        lp_input: Eip12InputBox {
            box_id: "lp_box_id".to_string(),
            transaction_id: "lp_tx_id".to_string(),
            index: 0,
            value: lp_erg.to_string(),
            ergo_tree: "lp_ergo_tree_hex".to_string(),
            assets: vec![
                Eip12Asset::new(LP_NFT_ID, 1),
                Eip12Asset::new(LP_TOKEN_ID, lp_token_reserves),
                Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
            ],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        lp_erg_reserves: lp_erg,
        lp_dexy_reserves: lp_dexy,
        lp_token_reserves,
        lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
        lp_box: dummy.clone(),
        lp_tokens: vec![
            (LP_NFT_ID.to_string(), 1),
            (LP_TOKEN_ID.to_string(), lp_token_reserves as u64),
            (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
        ],
        action_input: Eip12InputBox {
            box_id: "redeem_box_id".to_string(),
            transaction_id: "redeem_tx_id".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "redeem_ergo_tree_hex".to_string(),
            assets: vec![Eip12Asset::new(LP_REDEEM_NFT_ID, 1)],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        },
        action_erg_value: 1_000_000,
        action_ergo_tree: "redeem_ergo_tree_hex".to_string(),
        action_box: dummy,
        action_tokens: vec![(LP_REDEEM_NFT_ID.to_string(), 1)],
        oracle_data_input: Some(Eip12DataInputBox {
            box_id: "oracle_box_id".to_string(),
            transaction_id: "oracle_tx_id".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "oracle_ergo_tree_hex".to_string(),
            assets: vec![Eip12Asset::new(ORACLE_NFT_ID, 1)],
            creation_height: HEIGHT,
            additional_registers: HashMap::new(),
        }),
        oracle_rate_nano: Some(oracle_rate_nano),
    }
}

fn assert_eip12_field_eq(actual: &Eip12UnsignedTx, expected: &Value) {
    let actual_json = serde_json::to_string(actual).expect("serialize tx");
    let actual_v: Value = serde_json::from_str(&actual_json).expect("parse actual");
    let normalize = |v: &Value| -> Value {
        let mut obj = v.as_object().cloned().expect("tx object");
        for key in ["inputs", "dataInputs", "outputs"] {
            if let Some(Value::Array(arr)) = obj.get_mut(key) {
                for item in arr.iter_mut() {
                    if let Some(regs) = item.get_mut("additionalRegisters") {
                        *regs = sorted_map_value(regs);
                    }
                    if let Some(ext) = item.get_mut("extension") {
                        *ext = sorted_map_value(ext);
                    }
                }
            }
        }
        Value::Object(obj)
    };
    assert_eq!(normalize(&actual_v), normalize(expected));
}

fn sorted_map_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            serde_json::to_value(ordered).unwrap()
        }
        other => other.clone(),
    }
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_or_generate(name: &str, actual: &Eip12UnsignedTx) -> Value {
    let path = fixture_path(name);
    let actual_json = serde_json::to_string(actual).expect("serialize");
    if std::env::var("GENERATE_GOLDENS").is_ok() {
        let pretty = serde_json::to_string_pretty(
            &serde_json::from_str::<Value>(&actual_json).expect("actual json"),
        )
        .unwrap();
        std::fs::write(&path, pretty + "\n").expect("write fixture");
        eprintln!("wrote {}", path.display());
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {}", path.display(), e));
    serde_json::from_str(&text).expect("parse fixture")
}

#[test]
fn golden_build_mint_dexy_tx() {
    no_citadel_fee(|| {
        let ctx = mint_context();
        let state = mint_state();
        let request = MintDexyRequest {
            variant: DexyVariant::Gold,
            amount: 10,
            user_address: "user_address".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![user_input(100_000_000_000, vec![])],
            current_height: HEIGHT,
            recipient_ergo_tree: None,
        };
        let result = build_mint_dexy_tx(&request, &ctx, &state).expect("mint");
        let expected = load_or_generate("build_mint_dexy_tx.json", &result.unsigned_tx);
        assert_eip12_field_eq(&result.unsigned_tx, &expected);
    });
}

#[test]
fn golden_build_swap_dexy_tx_erg_to_dexy() {
    no_citadel_fee(|| {
        let ctx = swap_context(1_000_000_000_000, 1_000_000);
        let state = mint_state();
        let request = SwapDexyRequest {
            variant: DexyVariant::Gold,
            direction: SwapDirection::ErgToDexy,
            input_amount: 1_000_000_000,
            min_output: 1,
            user_address: "user_address".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![user_input(100_000_000_000, vec![])],
            current_height: HEIGHT,
            recipient_ergo_tree: None,
        };
        let result = build_swap_dexy_tx(&request, &ctx, &state).expect("swap");
        let expected = load_or_generate("build_swap_dexy_tx_erg_to_dexy.json", &result.unsigned_tx);
        assert_eip12_field_eq(&result.unsigned_tx, &expected);
    });
}

#[test]
fn golden_build_lp_deposit_tx() {
    no_citadel_fee(|| {
        let ctx = deposit_context(1_000_000_000_000, 500_000, 99_900_000_000);
        let request = LpDepositRequest {
            variant: DexyVariant::Gold,
            deposit_erg: 10_000_000_000,
            deposit_dexy: 5_000,
            user_address: "user_addr".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![user_input(
                100_000_000_000,
                vec![(DEXY_TOKEN_ID, 10_000)],
            )],
            current_height: HEIGHT,
            recipient_ergo_tree: None,
        };
        let result =
            build_lp_deposit_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP)
                .expect("deposit");
        let expected = load_or_generate("build_lp_deposit_tx.json", &result.unsigned_tx);
        assert_eip12_field_eq(&result.unsigned_tx, &expected);
    });
}

#[test]
fn golden_build_lp_redeem_tx() {
    no_citadel_fee(|| {
        // lp_rate = 1e12 / 500_000 = 2e6; oracle/divisor for gold = 220e9/1e6 = 220_000
        // 2e6 >> 98% of 220_000 → redeem allowed
        let ctx = redeem_context(1_000_000_000_000, 500_000, 99_900_000_000, 220_000_000_000);
        let request = LpRedeemRequest {
            variant: DexyVariant::Gold,
            lp_to_burn: 1_000_000,
            user_address: "user_addr".to_string(),
            user_ergo_tree: "user_ergo_tree".to_string(),
            user_inputs: vec![user_input(
                10_000_000_000,
                vec![(LP_TOKEN_ID, 5_000_000)],
            )],
            current_height: HEIGHT,
            recipient_ergo_tree: None,
        };
        let result =
            build_lp_redeem_tx(&request, &ctx, DEXY_TOKEN_ID, LP_TOKEN_ID, INITIAL_LP)
                .expect("redeem");
        let expected = load_or_generate("build_lp_redeem_tx.json", &result.unsigned_tx);
        assert_eip12_field_eq(&result.unsigned_tx, &expected);
    });
}
