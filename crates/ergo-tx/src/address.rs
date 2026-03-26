//! Ergo address utilities

/// Convert an Ergo address (base58) to its ErgoTree hex representation.
///
/// Tries both mainnet and testnet prefixes.
pub fn address_to_ergo_tree(address: &str) -> Result<String, AddressError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    for prefix in [NetworkPrefix::Mainnet, NetworkPrefix::Testnet] {
        let encoder = AddressEncoder::new(prefix);
        if let Ok(addr) = encoder.parse_address_from_str(address) {
            if let Ok(tree) = addr.script() {
                if let Ok(bytes) = tree.sigma_serialize_bytes() {
                    return Ok(hex::encode(bytes));
                }
            }
        }
    }

    Err(AddressError::InvalidAddress(address.to_string()))
}

/// Convert an ErgoTree hex to an Ergo address (mainnet).
pub fn ergo_tree_to_address(ergo_tree_hex: &str) -> Result<String, AddressError> {
    use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let tree_bytes = hex::decode(ergo_tree_hex)
        .map_err(|e| AddressError::InvalidErgoTree(format!("Invalid hex: {}", e)))?;

    let tree = ErgoTree::sigma_parse_bytes(&tree_bytes)
        .map_err(|e| AddressError::InvalidErgoTree(format!("Failed to parse: {}", e)))?;

    let address = Address::recreate_from_ergo_tree(&tree)
        .map_err(|e| AddressError::InvalidErgoTree(format!("Failed to create address: {}", e)))?;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

#[derive(Debug, thiserror::Error)]
pub enum AddressError {
    #[error("Invalid Ergo address: {0}")]
    InvalidAddress(String),
    #[error("Invalid ErgoTree: {0}")]
    InvalidErgoTree(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_mainnet_address() {
        let addr = "9hY16vzHmmfyVBwKeFGHvb2bMFsG94A1u7To1QWtUokACyFVENQ";
        let result = address_to_ergo_tree(addr);
        assert!(result.is_ok());
        let tree = result.unwrap();
        assert!(tree.starts_with("0008cd")); // P2PK prefix
    }

    #[test]
    fn test_invalid_address() {
        let result = address_to_ergo_tree("not_an_address");
        assert!(result.is_err());
    }
}
