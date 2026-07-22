//! Golden vectors for lending `build_*_tx` (Wave 3 Task 16).
//! Lock pre-split behavior; do not edit fixtures after split unless bugfix.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use lending::constants::get_pool;
use lending::state::CollateralOption;
use lending::tx_builder::{
    build_borrow_tx, build_lend_tx, build_refund_tx, build_repay_tx, build_withdraw_tx,
    BorrowRequest, LendRequest, ProxyBoxData, RepayRequest, UserUtxo, WithdrawRequest,
};
use serde_json::Value;

const TEST_ADDRESS: &str = "9hY16vzHmmfyVBwKeFGHvb2bMFsG94A1u7To1QWtUokACyFVENQ";
const HEIGHT: i32 = 1_000_000;

fn sample_utxo(box_id: &str, value: i64, assets: Vec<(String, i64)>) -> UserUtxo {
    UserUtxo {
        box_id: box_id.to_string(),
        tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
        index: 0,
        value,
        ergo_tree: "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f"
            .to_string(),
        assets,
        creation_height: 1000000,
        registers: HashMap::new(),
    }
}

fn sample_proxy_box(
    box_id: &str,
    value: i64,
    assets: Vec<(String, i64)>,
    refund_height: i64,
    is_repay_proxy: bool,
) -> ProxyBoxData {
    let user_ergo_tree =
        "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f".to_string();
    ProxyBoxData {
        box_id: box_id.to_string(),
        tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
        index: 0,
        value,
        ergo_tree: "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304".to_string(),
        assets,
        creation_height: 1000000,
        user_ergo_tree,
        r6_refund_height: refund_height,
        is_repay_proxy,
        additional_registers: HashMap::new(),
    }
}

/// Field-equal assertion for EIP-12 txs (Wave 3 golden policy).
fn assert_eip12_field_eq(actual_json: &str, expected: &Value) {
    let actual_v: Value = serde_json::from_str(actual_json).expect("parse actual tx json");
    let normalize = |v: &Value| -> Value {
        let mut obj = v.as_object().cloned().expect("tx object");
        for key in ["inputs", "dataInputs", "outputs"] {
            if let Some(Value::Array(arr)) = obj.get_mut(key) {
                for item in arr.iter_mut() {
                    if let Some(regs) = item.get_mut("additionalRegisters") {
                        *regs = sorted_map_value(regs);
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

fn load_or_generate(name: &str, actual_json: &str) -> Value {
    let path = fixture_path(name);
    if std::env::var("GENERATE_GOLDENS").is_ok() {
        let pretty = serde_json::to_string_pretty(
            &serde_json::from_str::<Value>(actual_json).expect("actual json"),
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
fn golden_build_lend_tx_erg_pool() {
    let config = get_pool("erg").unwrap();
    let amount: u64 = 10_000_000_000;
    let req = LendRequest {
        pool_id: "erg".to_string(),
        amount,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo(&"a".repeat(64), 15_000_000_000, vec![])],
        min_lp_tokens: Some(100),
        slippage_bps: 0,
    };
    let response = build_lend_tx(req, config, HEIGHT).expect("lend");
    let expected = load_or_generate("build_lend_tx_erg.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}

#[test]
fn golden_build_withdraw_tx_erg_pool() {
    let config = get_pool("erg").unwrap();
    let lp_token_id = config.lend_token_id.to_string();
    let req = WithdrawRequest {
        pool_id: "erg".to_string(),
        lp_amount: 1000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo(
            &"b".repeat(64),
            10_000_000_000,
            vec![(lp_token_id, 5000)],
        )],
        min_output: Some(9_000_000_000),
    };
    let response = build_withdraw_tx(req, config, HEIGHT).expect("withdraw");
    let expected = load_or_generate("build_withdraw_tx_erg.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}

#[test]
fn golden_build_repay_tx_erg_pool() {
    let config = get_pool("erg").unwrap();
    let repay_amount: u64 = 5_000_000_000;
    let req = RepayRequest {
        pool_id: "erg".to_string(),
        collateral_box_id: "a".repeat(64),
        repay_amount,
        total_owed: repay_amount,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo(&"d".repeat(64), 10_000_000_000, vec![])],
    };
    let response = build_repay_tx(req, config, HEIGHT).expect("repay");
    let expected = load_or_generate("build_repay_tx_erg.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}

#[test]
fn golden_build_borrow_tx_sigusd_native_collateral() {
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
    let req = BorrowRequest {
        pool_id: "sigusd".to_string(),
        collateral_token: "native".to_string(),
        collateral_amount: 10_000_000_000,
        borrow_amount: 10_000,
        user_address: TEST_ADDRESS.to_string(),
        user_utxos: vec![sample_utxo(&"h".repeat(64), 15_000_000_000, vec![])],
    };
    let response = build_borrow_tx(req, config, &collateral_config, HEIGHT).expect("borrow");
    let expected = load_or_generate("build_borrow_tx_sigusd.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}

#[test]
fn golden_build_refund_tx_basic() {
    let proxy = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], 1_000_720, false);
    let response = build_refund_tx(proxy, 1_001_000).expect("refund");
    let expected = load_or_generate("build_refund_tx_basic.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}

#[test]
fn golden_build_refund_tx_repay_proxy() {
    let mut proxy = sample_proxy_box(&"c".repeat(64), 10_000_000_000, vec![], 1_000_720, true);
    proxy.is_repay_proxy = true;
    let response = build_refund_tx(proxy, 1_001_000).expect("refund repay");
    let expected = load_or_generate("build_refund_tx_repay_proxy.json", &response.unsigned_tx);
    assert_eip12_field_eq(&response.unsigned_tx, &expected);
}
