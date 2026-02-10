//! Basic address validation for target chains

/// Validate a destination address for a specific target chain.
///
/// Performs basic format checks (prefix, length) — not full cryptographic validation.
pub fn validate_target_address(chain: &str, address: &str) -> Result<(), String> {
    if address.is_empty() {
        return Err("Address cannot be empty".to_string());
    }

    match chain {
        "cardano" => {
            if !address.starts_with("addr1") {
                return Err("Cardano address must start with 'addr1'".to_string());
            }
            if address.len() < 50 {
                return Err("Cardano address is too short".to_string());
            }
        }
        "bitcoin" => {
            if !address.starts_with("bc1q") {
                return Err("Bitcoin address must start with 'bc1q' (SegWit bech32)".to_string());
            }
            if address.len() < 40 || address.len() > 62 {
                return Err("Invalid Bitcoin address length".to_string());
            }
        }
        "bitcoin-runes" => {
            if !address.starts_with("bc1p") {
                return Err(
                    "Bitcoin Runes address must start with 'bc1p' (Taproot bech32m)".to_string(),
                );
            }
            if address.len() < 40 || address.len() > 62 {
                return Err("Invalid Bitcoin Runes address length".to_string());
            }
        }
        "ethereum" | "binance" => {
            if !address.starts_with("0x") {
                return Err(format!(
                    "{} address must start with '0x'",
                    if chain == "ethereum" {
                        "Ethereum"
                    } else {
                        "Binance"
                    }
                ));
            }
            if address.len() != 42 {
                return Err(format!(
                    "{} address must be 42 characters (0x + 40 hex chars)",
                    if chain == "ethereum" {
                        "Ethereum"
                    } else {
                        "Binance"
                    }
                ));
            }
            // Check hex characters after 0x
            if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("Address contains non-hex characters".to_string());
            }
        }
        "doge" => {
            if !address.starts_with('D') {
                return Err("Dogecoin address must start with 'D'".to_string());
            }
            if address.len() < 25 || address.len() > 34 {
                return Err("Invalid Dogecoin address length".to_string());
            }
        }
        _ => {
            return Err(format!("Unsupported target chain: {}", chain));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardano_valid() {
        let addr = "addr1qxck39mfuzd4tcamp02gycm7aqnlhkxskvfjxhe0ekmzp8lrstxkxqyer6vk6g3emeqyqsghx09gvpqx9fhsgqx6wlqyu66ts";
        assert!(validate_target_address("cardano", addr).is_ok());
    }

    #[test]
    fn test_cardano_invalid_prefix() {
        assert!(validate_target_address("cardano", "stake1u8test").is_err());
    }

    #[test]
    fn test_bitcoin_valid() {
        let addr = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        assert!(validate_target_address("bitcoin", addr).is_ok());
    }

    #[test]
    fn test_bitcoin_invalid_prefix() {
        assert!(validate_target_address("bitcoin", "1A1zP1test").is_err());
    }

    #[test]
    fn test_bitcoin_runes_valid() {
        let addr = "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297";
        assert!(validate_target_address("bitcoin-runes", addr).is_ok());
    }

    #[test]
    fn test_ethereum_valid() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD08";
        assert!(validate_target_address("ethereum", addr).is_ok());
    }

    #[test]
    fn test_ethereum_invalid_length() {
        assert!(validate_target_address("ethereum", "0x742d35").is_err());
    }

    #[test]
    fn test_binance_valid() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD08";
        assert!(validate_target_address("binance", addr).is_ok());
    }

    #[test]
    fn test_doge_valid() {
        let addr = "D8kGdYdLjP5wMngiPWMNhkMh4u";
        assert!(validate_target_address("doge", addr).is_ok());
    }

    #[test]
    fn test_doge_invalid_prefix() {
        assert!(validate_target_address("doge", "L8kGdYd").is_err());
    }

    #[test]
    fn test_empty_address() {
        assert!(validate_target_address("cardano", "").is_err());
    }

    #[test]
    fn test_unsupported_chain() {
        assert!(validate_target_address("solana", "some_address").is_err());
    }
}
