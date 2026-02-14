//! Shared utilities for parsing ErgoBox registers and tokens.
//!
//! Consolidates common extraction patterns used across protocol crates
//! (sigmausd, dexy, lending, hodlcoin).

use citadel_core::{NodeError, ProtocolError};
use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
use ergo_lib::ergotree_ir::types::stype::SType;

// =============================================================================
// Register value extractors
// =============================================================================

/// Extract an `i64` from an ergo-lib `Constant` that holds a `Long` literal.
pub fn extract_long(constant: &Constant) -> Result<i64, String> {
    match &constant.v {
        Literal::Long(val) => Ok(*val),
        other => Err(format!("Expected Long, got {:?}", other)),
    }
}

/// Extract an `i32` from an ergo-lib `Constant` that holds an `Int` literal.
pub fn extract_int(constant: &Constant) -> Result<i32, String> {
    match &constant.v {
        Literal::Int(val) => Ok(*val),
        other => Err(format!("Expected Int, got {:?}", other)),
    }
}

/// Extract `(i32, i32)` from a `Constant` holding a `Tup(Int, Int)`.
pub fn extract_int_pair(constant: &Constant) -> Result<(i32, i32), String> {
    match &constant.v {
        Literal::Tup(items) if items.len() == 2 => {
            let a = match &items.as_slice()[0] {
                Literal::Int(v) => *v,
                other => return Err(format!("Expected Int in tuple[0], got {:?}", other)),
            };
            let b = match &items.as_slice()[1] {
                Literal::Int(v) => *v,
                other => return Err(format!("Expected Int in tuple[1], got {:?}", other)),
            };
            Ok((a, b))
        }
        other => Err(format!("Expected Tup(Int, Int), got {:?}", other)),
    }
}

/// Extract `Vec<i64>` from an ergo-lib `Constant` that holds a `Coll[Long]`.
pub fn extract_long_coll(constant: &Constant) -> Result<Vec<i64>, String> {
    match &constant.v {
        Literal::Coll(coll) => match coll {
            CollKind::WrappedColl {
                elem_tpe: SType::SLong,
                items,
            } => {
                let mut result = Vec::new();
                for item in items.iter() {
                    match item {
                        Literal::Long(v) => result.push(*v),
                        other => return Err(format!("Expected Long in Coll, got {:?}", other)),
                    }
                }
                Ok(result)
            }
            _ => Err(format!("Expected Coll[Long], got {:?}", coll)),
        },
        other => Err(format!("Expected Coll literal, got {:?}", other)),
    }
}

/// Extract `Vec<Vec<u8>>` from an ergo-lib `Constant` that holds a `Coll[Coll[Byte]]`.
///
/// Used for registers containing arrays of byte arrays, such as token IDs or DEX NFT IDs
/// in Duckpools parameter boxes.
pub fn extract_byte_array_coll(constant: &Constant) -> Result<Vec<Vec<u8>>, String> {
    match &constant.v {
        Literal::Coll(coll) => match coll {
            CollKind::WrappedColl {
                elem_tpe: SType::SColl(inner),
                items,
            } if **inner == SType::SByte => {
                let mut result = Vec::new();
                for item in items.iter() {
                    match item {
                        Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes))) => {
                            result.push(bytes.iter().map(|&b| b as u8).collect());
                        }
                        other => {
                            return Err(format!(
                                "Expected Coll[Byte] in Coll[Coll[Byte]], got {:?}",
                                other
                            ))
                        }
                    }
                }
                Ok(result)
            }
            _ => Err(format!("Expected Coll[Coll[Byte]], got {:?}", coll)),
        },
        other => Err(format!("Expected Coll literal, got {:?}", other)),
    }
}

/// Read a Long register value from an ErgoBox, returning `None` on any failure.
///
/// Combines `get_constant` + `extract_long` in one call (hodlcoin pattern).
pub fn read_register_long(ergo_box: &ErgoBox, reg: NonMandatoryRegisterId) -> Option<i64> {
    ergo_box
        .additional_registers
        .get_constant(reg)
        .ok()
        .flatten()
        .and_then(|c| match &c.v {
            Literal::Long(v) => Some(*v),
            _ => None,
        })
}

// =============================================================================
// Token extraction
// =============================================================================

/// Find a token's amount in a box by its hex token ID.
///
/// Returns `None` if the box has no tokens or the token is not found.
/// Callers decide whether missing means 0 or an error.
pub fn find_token_amount(ergo_box: &ErgoBox, token_id: &str) -> Option<u64> {
    ergo_box.tokens.as_ref().and_then(|tokens| {
        tokens.iter().find_map(|t| {
            let tid: String = t.token_id.into();
            if tid == token_id {
                Some(*t.amount.as_u64())
            } else {
                None
            }
        })
    })
}

/// Get the token amount at a specific index in the box's token list.
///
/// Returns `None` if the box has no tokens or the index is out of bounds.
pub fn token_at_index(ergo_box: &ErgoBox, index: usize) -> Option<u64> {
    ergo_box
        .tokens
        .as_ref()
        .and_then(|tokens| tokens.get(index))
        .map(|token| *token.amount.as_u64())
}

// =============================================================================
// Node error mapping
// =============================================================================

/// Map a `NodeError` to a `ProtocolError` with a standard pattern.
///
/// - `ExtraIndexRequired` → `StateUnavailable` with protocol-specific message
/// - All others → `BoxParseError` with context
pub fn map_node_error(err: NodeError, protocol_name: &str, context: &str) -> ProtocolError {
    match err {
        NodeError::ExtraIndexRequired { .. } => ProtocolError::StateUnavailable {
            reason: format!(
                "{} requires an indexed node with extraIndex enabled",
                protocol_name
            ),
        },
        _ => ProtocolError::BoxParseError {
            message: format!("{} not found: {}", context, err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_long_ok() {
        let constant = Constant {
            tpe: SType::SLong,
            v: Literal::Long(1_000_000_000),
        };
        assert_eq!(extract_long(&constant).unwrap(), 1_000_000_000);
    }

    #[test]
    fn test_extract_long_wrong_type() {
        let constant = Constant {
            tpe: SType::SInt,
            v: Literal::Int(42),
        };
        let err = extract_long(&constant).unwrap_err();
        assert!(err.contains("Expected Long"));
    }

    #[test]
    fn test_extract_int_ok() {
        let constant = Constant {
            tpe: SType::SInt,
            v: Literal::Int(123),
        };
        assert_eq!(extract_int(&constant).unwrap(), 123);
    }

    #[test]
    fn test_extract_int_wrong_type() {
        let constant = Constant {
            tpe: SType::SLong,
            v: Literal::Long(99),
        };
        let err = extract_int(&constant).unwrap_err();
        assert!(err.contains("Expected Int"));
    }

    #[test]
    fn test_extract_long_coll_ok() {
        use std::sync::Arc;
        let items: Arc<[Literal]> =
            vec![Literal::Long(10), Literal::Long(20), Literal::Long(30)].into();
        let constant = Constant {
            tpe: SType::SColl(Arc::new(SType::SLong)),
            v: Literal::Coll(CollKind::WrappedColl {
                elem_tpe: SType::SLong,
                items,
            }),
        };
        assert_eq!(extract_long_coll(&constant).unwrap(), vec![10, 20, 30]);
    }

    #[test]
    fn test_extract_long_coll_empty() {
        use std::sync::Arc;
        let items: Arc<[Literal]> = vec![].into();
        let constant = Constant {
            tpe: SType::SColl(Arc::new(SType::SLong)),
            v: Literal::Coll(CollKind::WrappedColl {
                elem_tpe: SType::SLong,
                items,
            }),
        };
        assert_eq!(extract_long_coll(&constant).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn test_extract_byte_array_coll_ok() {
        use std::sync::Arc;
        let inner_items: Vec<Literal> = vec![
            Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(
                vec![1i8, 2, 3].into(),
            ))),
            Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(
                vec![4i8, 5].into(),
            ))),
        ];
        let constant = Constant {
            tpe: SType::SColl(Arc::new(SType::SColl(Arc::new(SType::SByte)))),
            v: Literal::Coll(CollKind::WrappedColl {
                elem_tpe: SType::SColl(Arc::new(SType::SByte)),
                items: inner_items.into(),
            }),
        };
        let result = extract_byte_array_coll(&constant).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![1u8, 2, 3]);
        assert_eq!(result[1], vec![4u8, 5]);
    }

    #[test]
    fn test_extract_byte_array_coll_empty() {
        use std::sync::Arc;
        let constant = Constant {
            tpe: SType::SColl(Arc::new(SType::SColl(Arc::new(SType::SByte)))),
            v: Literal::Coll(CollKind::WrappedColl {
                elem_tpe: SType::SColl(Arc::new(SType::SByte)),
                items: vec![].into(),
            }),
        };
        let result = extract_byte_array_coll(&constant).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_map_node_error_extra_index() {
        let err = NodeError::ExtraIndexRequired {
            feature: "token search",
        };
        let result = map_node_error(err, "SigmaUSD", "Bank box");
        match result {
            ProtocolError::StateUnavailable { reason } => {
                assert!(reason.contains("SigmaUSD"));
                assert!(reason.contains("extraIndex"));
            }
            _ => panic!("Expected StateUnavailable"),
        }
    }

    #[test]
    fn test_map_node_error_other() {
        let err = NodeError::BoxNotFound {
            box_id: "abc123".to_string(),
        };
        let result = map_node_error(err, "Dexy", "Bank box");
        match result {
            ProtocolError::BoxParseError { message } => {
                assert!(message.contains("Bank box not found"));
            }
            _ => panic!("Expected BoxParseError"),
        }
    }
}
