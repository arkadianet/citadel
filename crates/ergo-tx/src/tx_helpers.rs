//! Shared transaction building helpers.

use crate::box_selector::{self, BoxSelectorError, SelectedInputs};
use crate::eip12::{Eip12InputBox, Eip12Output};

pub const MIN_CHANGE_VALUE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct ChangeOutputError {
    pub min_value: u64,
    pub available: u64,
}

impl std::fmt::Display for ChangeOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Change tokens exist but not enough ERG for change box (need {}, have {})",
            self.min_value, self.available
        )
    }
}

impl std::error::Error for ChangeOutputError {}

pub fn append_change_output(
    outputs: &mut Vec<Eip12Output>,
    selected: &SelectedInputs,
    erg_used: u64,
    spent_tokens: &[(&str, u64)],
    user_ergo_tree: &str,
    current_height: i32,
    min_change_value: u64,
) -> Result<(), ChangeOutputError> {
    let change_erg = selected.total_erg.saturating_sub(erg_used);

    let change_tokens = if spent_tokens.len() == 1 {
        box_selector::collect_change_tokens(
            &selected.boxes,
            Some((spent_tokens[0].0, spent_tokens[0].1)),
        )
    } else if spent_tokens.is_empty() {
        box_selector::collect_change_tokens(&selected.boxes, None)
    } else {
        box_selector::collect_multi_change_tokens(&selected.boxes, spent_tokens)
    };

    if !change_tokens.is_empty() && change_erg < min_change_value {
        return Err(ChangeOutputError {
            min_value: min_change_value,
            available: change_erg,
        });
    }

    if change_erg >= min_change_value || !change_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            change_erg as i64,
            user_ergo_tree,
            change_tokens,
            current_height,
        ));
    }

    Ok(())
}

pub fn select_inputs_for_spend(
    utxos: &[Eip12InputBox],
    required_erg: u64,
    token: Option<(&str, u64)>,
) -> Result<SelectedInputs, BoxSelectorError> {
    match token {
        Some((token_id, amount)) => {
            box_selector::select_token_boxes(utxos, token_id, amount, required_erg)
        }
        None => box_selector::select_erg_boxes(utxos, required_erg),
    }
}

pub fn select_inputs_for_multi_spend(
    utxos: &[Eip12InputBox],
    required_erg: u64,
    tokens: &[(&str, u64)],
) -> Result<SelectedInputs, BoxSelectorError> {
    match tokens.len() {
        0 => box_selector::select_erg_boxes(utxos, required_erg),
        1 => box_selector::select_token_boxes(utxos, tokens[0].0, tokens[0].1, required_erg),
        _ => box_selector::select_multi_token_boxes(utxos, tokens, required_erg),
    }
}

pub fn fee_output(fee: i64, height: i32) -> Eip12Output {
    Eip12Output::fee(fee, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eip12::Eip12Asset;
    use std::collections::HashMap;

    fn mock_utxo(box_id: &str, value: u64, assets: Vec<(&str, u64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: box_id.to_string(),
            transaction_id: "tx123".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
            assets: assets
                .into_iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: id.to_string(),
                    amount: amt.to_string(),
                })
                .collect(),
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_append_change_output_erg_only() {
        let selected = SelectedInputs {
            boxes: vec![mock_utxo("box1", 5_000_000_000, vec![])],
            total_erg: 5_000_000_000,
            token_amount: 0,
        };

        let mut outputs = vec![];
        append_change_output(
            &mut outputs,
            &selected,
            3_000_000_000,
            &[],
            "0008cd...",
            1000,
            MIN_CHANGE_VALUE,
        )
        .unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].value, "2000000000");
    }

    #[test]
    fn test_append_change_output_no_change_needed() {
        let selected = SelectedInputs {
            boxes: vec![mock_utxo("box1", 2_100_000, vec![])],
            total_erg: 2_100_000,
            token_amount: 0,
        };

        let mut outputs = vec![];
        append_change_output(
            &mut outputs,
            &selected,
            2_100_000,
            &[],
            "0008cd...",
            1000,
            MIN_CHANGE_VALUE,
        )
        .unwrap();

        assert_eq!(outputs.len(), 0);
    }

    #[test]
    fn test_append_change_output_with_tokens() {
        let selected = SelectedInputs {
            boxes: vec![mock_utxo("box1", 5_000_000_000, vec![("tokenA", 100)])],
            total_erg: 5_000_000_000,
            token_amount: 100,
        };

        let mut outputs = vec![];
        append_change_output(
            &mut outputs,
            &selected,
            3_000_000_000,
            &[("tokenA", 60)],
            "0008cd...",
            1000,
            MIN_CHANGE_VALUE,
        )
        .unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].assets.len(), 1);
        assert_eq!(outputs[0].assets[0].amount, "40");
    }

    #[test]
    fn test_append_change_output_error_tokens_without_erg() {
        let selected = SelectedInputs {
            boxes: vec![mock_utxo("box1", 2_100_000, vec![("tokenA", 100)])],
            total_erg: 2_100_000,
            token_amount: 100,
        };

        let result = append_change_output(
            &mut vec![],
            &selected,
            2_100_000, // all ERG used, 0 change
            &[("tokenA", 50)], // but 50 tokens left over
            "0008cd...",
            1000,
            MIN_CHANGE_VALUE,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_select_inputs_for_spend_erg() {
        let utxos = vec![mock_utxo("box1", 5_000_000_000, vec![])];
        let result = select_inputs_for_spend(&utxos, 2_000_000_000, None).unwrap();
        assert_eq!(result.total_erg, 5_000_000_000);
    }

    #[test]
    fn test_select_inputs_for_spend_token() {
        let utxos = vec![mock_utxo("box1", 5_000_000_000, vec![("tokenA", 100)])];
        let result =
            select_inputs_for_spend(&utxos, 1_000_000_000, Some(("tokenA", 50))).unwrap();
        assert_eq!(result.token_amount, 100);
    }

    #[test]
    fn test_select_inputs_for_multi_spend() {
        let utxos = vec![
            mock_utxo("box1", 3_000_000_000, vec![("tokenA", 100)]),
            mock_utxo("box2", 2_000_000_000, vec![("tokenB", 200)]),
        ];
        let result = select_inputs_for_multi_spend(
            &utxos,
            1_000_000_000,
            &[("tokenA", 50), ("tokenB", 100)],
        )
        .unwrap();
        assert_eq!(result.boxes.len(), 2);
    }
}
