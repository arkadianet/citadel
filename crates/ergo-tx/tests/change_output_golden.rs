//! Golden vectors for change-output helpers (Wave 3 Task 14).
//! Lock pre-extract behavior; do not edit fixtures after extract unless bugfix.

use ergo_tx::{Eip12Asset, Eip12Output, Eip12UnsignedTx};
use serde_json::Value;
use std::collections::BTreeMap;

/// Field-equal assertion for EIP-12 txs (Wave 3 golden policy).
fn assert_eip12_field_eq(actual: &Eip12UnsignedTx, expected: &Value) {
    let actual_v = serde_json::to_value(actual).expect("serialize actual tx");
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

#[test]
fn change_output_matches_fixture() {
    let assets = vec![Eip12Asset::new("aa".repeat(32), 10)];
    let out = Eip12Output::change(1_000_000, "0008cd11", assets, 1_000_000);
    let value = serde_json::to_value(&out).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/change_output_basic.json")).unwrap();
    assert_eq!(value, expected);
}

#[test]
fn change_output_empty_assets_matches_fixture() {
    let out = Eip12Output::change(
        2_500_000,
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        vec![],
        42,
    );
    let value = serde_json::to_value(&out).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/change_output_empty_assets.json")).unwrap();
    assert_eq!(value, expected);
}

#[test]
fn append_change_output_matches_fixture() {
    use ergo_tx::{append_change_output, select_erg_boxes};
    use std::collections::HashMap;

    let utxo = ergo_tx::Eip12InputBox {
        box_id: "box1".into(),
        transaction_id: "tx1".into(),
        index: 0,
        value: "5_000_000".replace('_', ""),
        ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
            .into(),
        assets: vec![Eip12Asset::new("bb".repeat(32), 7)],
        creation_height: 100,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    };
    let selected = select_erg_boxes(std::slice::from_ref(&utxo), 1_100_000).unwrap();
    let mut outputs = vec![Eip12Output::simple(1_000_000, "0008cd11", 200)];
    append_change_output(
        &mut outputs,
        &selected,
        1_100_000,
        &[],
        &utxo.ergo_tree,
        200,
        1_000_000,
    )
    .unwrap();
    outputs.push(Eip12Output::fee(1_100_000, 200));

    let tx = Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    };
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/append_change_output_basic.json")).unwrap();
    assert_eip12_field_eq(&tx, &expected);
}
