//! Golden vectors for AMM router + direct_swap (Wave 3 Task 18).
//! Lock pre-split behavior; do not edit fixtures after split unless bugfix.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use amm::direct_swap::build_direct_swap_eip12;
use amm::router::{build_pool_graph, find_best_routes, make_route_quote, ERG_TOKEN_ID};
use amm::state::{AmmPool, PoolType, SwapInput, TokenAmount};
use ergo_tx::{with_test_dev_fee, DevFeeConfig, Eip12Asset, Eip12InputBox, Eip12UnsignedTx};
use serde_json::Value;

const HEIGHT: i32 = 1_000_000;
const USER_TREE: &str = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

fn no_citadel_fee<R>(f: impl FnOnce() -> R) -> R {
    with_test_dev_fee(DevFeeConfig::disabled(), f)
}

fn make_n2t_pool(
    pool_id: &str,
    erg_reserves: u64,
    token_id: &str,
    token_name: &str,
    token_reserves: u64,
    fee_num: i32,
) -> AmmPool {
    AmmPool {
        pool_id: pool_id.to_string(),
        pool_type: PoolType::N2T,
        box_id: format!("box_{}", pool_id),
        erg_reserves: Some(erg_reserves),
        token_x: None,
        token_y: TokenAmount {
            token_id: token_id.to_string(),
            amount: token_reserves,
            decimals: Some(2),
            name: Some(token_name.to_string()),
        },
        lp_token_id: format!("lp_{}", pool_id),
        lp_circulating: 1000,
        fee_num,
        fee_denom: 1000,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_t2t_pool(
    pool_id: &str,
    x_id: &str,
    x_name: &str,
    x_amount: u64,
    y_id: &str,
    y_name: &str,
    y_amount: u64,
    fee_num: i32,
) -> AmmPool {
    AmmPool {
        pool_id: pool_id.to_string(),
        pool_type: PoolType::T2T,
        box_id: format!("box_{}", pool_id),
        erg_reserves: Some(600_000),
        token_x: Some(TokenAmount {
            token_id: x_id.to_string(),
            amount: x_amount,
            decimals: Some(2),
            name: Some(x_name.to_string()),
        }),
        token_y: TokenAmount {
            token_id: y_id.to_string(),
            amount: y_amount,
            decimals: Some(2),
            name: Some(y_name.to_string()),
        },
        lp_token_id: format!("lp_{}", pool_id),
        lp_circulating: 1000,
        fee_num,
        fee_denom: 1000,
    }
}

fn test_n2t_pool() -> AmmPool {
    AmmPool {
        pool_id: "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        pool_type: PoolType::N2T,
        box_id: "pool_box_1".to_string(),
        erg_reserves: Some(100_000_000_000),
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
        value: "100000000000".to_string(),
        ergo_tree: "pool_ergo_tree_hex".to_string(),
        assets: vec![
            Eip12Asset {
                token_id: "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "1".to_string(),
            },
            Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "9223372036854774807".to_string(),
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
            // fee_num=997 (sigma Int) — R4 is ground truth for direct swap
            m.insert("R4".to_string(), "04ca0f".to_string());
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
        value: "10000000000".to_string(),
        ergo_tree: USER_TREE.to_string(),
        assets: vec![],
        creation_height: 999_000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
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

fn normalize_eip12(v: &Value) -> Value {
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
}

fn assert_json_field_eq(actual: &Value, expected: &Value) {
    assert_eq!(actual, expected);
}

fn assert_eip12_field_eq(actual: &Eip12UnsignedTx, expected: &Value) {
    let actual_json = serde_json::to_string(actual).expect("serialize tx");
    let actual_v: Value = serde_json::from_str(&actual_json).expect("parse actual");
    assert_eq!(normalize_eip12(&actual_v), normalize_eip12(expected));
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_or_generate(name: &str, actual: &Value) -> Value {
    let path = fixture_path(name);
    if std::env::var("GENERATE_GOLDENS").is_ok() {
        let pretty = serde_json::to_string_pretty(actual).unwrap();
        std::fs::write(&path, pretty + "\n").expect("write fixture");
        eprintln!("wrote {}", path.display());
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {}", path.display(), e));
    serde_json::from_str(&text).expect("parse fixture")
}

/// One multi-hop router path: ERG -> gort -> sigusd via find_best_routes + make_route_quote.
#[test]
fn golden_router_best_multihop_quote() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "gort", "GORT", 50_000, 997),
        make_t2t_pool(
            "t2t_1", "gort", "GORT", 10_000, "sigusd", "SigUSD", 5_000, 995,
        ),
    ];
    let graph = build_pool_graph(&pools, 0);
    let routes = find_best_routes(&graph, ERG_TOKEN_ID, "sigusd", 1_000_000_000, 3, 5);
    assert!(!routes.is_empty(), "expected at least one route");
    let quote = make_route_quote(routes[0].clone(), 0.5);
    // fee_num must come from pool (995 on T2T hop), never hardcoded 997
    assert_eq!(quote.route.hops[1].fee_num, 995);

    let actual = serde_json::to_value(&quote).expect("serialize RouteQuote");
    let expected = load_or_generate("router_best_multihop_quote.json", &actual);
    assert_json_field_eq(&actual, &expected);
}

/// One direct swap path: N2T ERG->token with fee_num from R4.
#[test]
fn golden_direct_swap_erg_to_token() {
    no_citadel_fee(|| {
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
            1, // min_output loose — golden locks exact outputs
            &[user_utxo],
            USER_TREE,
            HEIGHT,
            None,
            None,
        )
        .expect("build direct swap");

        let actual = serde_json::to_value(&result.unsigned_tx).expect("serialize tx");
        let expected = load_or_generate("direct_swap_erg_to_token.json", &actual);
        assert_eip12_field_eq(&result.unsigned_tx, &expected);

        let summary = serde_json::to_value(&result.summary).expect("serialize summary");
        let expected_summary = load_or_generate("direct_swap_erg_to_token_summary.json", &summary);
        assert_json_field_eq(&summary, &expected_summary);
    });
}
