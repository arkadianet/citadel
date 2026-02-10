//! Sigma register encoding/decoding utilities
//!
//! Ergo registers use Sigma serialization format:
//! - Type tag (1 byte): 0x05 for Long
//! - VLQ zigzag encoded value

/// Encode an i32 value as a Sigma Int register value (hex string)
/// Format: 0x04 (SInt type tag) + VLQ zigzag encoded value
pub fn encode_sigma_int(value: i32) -> String {
    let mut bytes = vec![0x04u8]; // SInt type tag
    let zigzag = ((value << 1) ^ (value >> 31)) as u32;
    vlq_encode(&mut bytes, zigzag as u64);
    hex::encode(bytes)
}

/// Encode an i64 value as a Sigma Long register value (hex string)
/// Format: 0x05 (SLong type tag) + VLQ zigzag encoded value
pub fn encode_sigma_long(value: i64) -> String {
    let mut bytes = vec![0x05u8]; // SLong type tag

    // Zigzag encode for signed value using bitwise operations
    // This handles i64::MIN correctly (arithmetic -value would overflow)
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;

    // VLQ encode
    let mut n = zigzag;
    loop {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80; // Set continuation bit
        }
        bytes.push(byte);
        if n == 0 {
            break;
        }
    }

    hex::encode(bytes)
}

/// Decode a Sigma Long from register hex string
/// Format: 0x05 (type tag) + VLQ zigzag encoded value
pub fn decode_sigma_long(hex_str: &str) -> Result<i64, SigmaDecodeError> {
    let bytes = hex::decode(hex_str).map_err(|_| SigmaDecodeError::InvalidHex)?;

    if bytes.is_empty() {
        return Err(SigmaDecodeError::EmptyInput);
    }

    if bytes[0] != 0x05 {
        return Err(SigmaDecodeError::InvalidTypeTag {
            expected: 0x05,
            found: bytes[0],
        });
    }

    // VLQ decode
    let mut result: u64 = 0;
    let mut shift = 0;
    for &byte in &bytes[1..] {
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return Err(SigmaDecodeError::Overflow);
        }
    }

    // Zigzag decode
    let value = if result & 1 == 0 {
        (result >> 1) as i64
    } else {
        -((result >> 1) as i64) - 1
    };

    Ok(value)
}

/// Encode a GroupElement (compressed EC point) as a Sigma register value (hex string).
///
/// Sigma serialization format:
/// ```text
/// 0x07      -- type descriptor: SGroupElement
/// <33 bytes> -- compressed EC point (02/03 prefix + 32 bytes X coordinate)
/// ```
///
/// Used by MewLock to store the depositor's public key in R4.
pub fn encode_sigma_group_element(pubkey: &[u8; 33]) -> String {
    let mut bytes = vec![0x07u8]; // SGroupElement type tag
    bytes.extend_from_slice(pubkey);
    hex::encode(bytes)
}

/// Encode a `(Long, Long)` tuple as a Sigma register value (hex string).
///
/// Sigma serialization format (confirmed from Python `off-chain-bot` serializer):
/// ```text
/// 0x59                 -- type descriptor: (SLong, SLong) tuple
/// <zigzag-VLQ(a)>     -- first Long value (bare, no 0x05 type tag)
/// <zigzag-VLQ(b)>     -- second Long value (bare, no 0x05 type tag)
/// ```
///
/// Used by Duckpools borrow proxy R7 for `(threshold, penalty)` pair.
pub fn encode_sigma_long_pair(a: i64, b: i64) -> String {
    let mut bytes = vec![0x59u8]; // (SLong, SLong) type descriptor

    // Zigzag + VLQ encode each value (bare, without the 0x05 type tag)
    let zigzag_a = ((a << 1) ^ (a >> 63)) as u64;
    vlq_encode(&mut bytes, zigzag_a);

    let zigzag_b = ((b << 1) ^ (b >> 63)) as u64;
    vlq_encode(&mut bytes, zigzag_b);

    hex::encode(bytes)
}

/// Extract the 33-byte compressed public key from a P2PK ErgoTree.
///
/// P2PK ErgoTree format: `0008cd` + 33-byte SEC1 compressed public key.
/// Returns the raw 33 bytes suitable for `encode_sigma_group_element`.
pub fn extract_pk_from_p2pk_ergo_tree(ergo_tree_hex: &str) -> Result<[u8; 33], SigmaDecodeError> {
    let bytes = hex::decode(ergo_tree_hex).map_err(|_| SigmaDecodeError::InvalidHex)?;

    // P2PK ErgoTree = 0x00 0x08 0xcd + 33 bytes = 36 bytes total
    if bytes.len() != 36 {
        return Err(SigmaDecodeError::InvalidLength {
            expected: 36,
            found: bytes.len(),
        });
    }

    if bytes[0] != 0x00 || bytes[1] != 0x08 || bytes[2] != 0xcd {
        return Err(SigmaDecodeError::InvalidTypeTag {
            expected: 0xcd,
            found: bytes[2],
        });
    }

    let mut pk = [0u8; 33];
    pk.copy_from_slice(&bytes[3..36]);
    Ok(pk)
}

/// Decode a GroupElement from a Sigma register hex string.
///
/// Strips the `0x07` type tag prefix and returns the 33-byte compressed EC point.
pub fn decode_sigma_group_element(hex_str: &str) -> Result<[u8; 33], SigmaDecodeError> {
    let bytes = hex::decode(hex_str).map_err(|_| SigmaDecodeError::InvalidHex)?;

    if bytes.is_empty() {
        return Err(SigmaDecodeError::EmptyInput);
    }

    if bytes[0] != 0x07 {
        return Err(SigmaDecodeError::InvalidTypeTag {
            expected: 0x07,
            found: bytes[0],
        });
    }

    // Must be exactly 1 (type tag) + 33 (compressed point) = 34 bytes
    if bytes.len() != 34 {
        return Err(SigmaDecodeError::InvalidLength {
            expected: 34,
            found: bytes.len(),
        });
    }

    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&bytes[1..34]);
    Ok(pubkey)
}

/// Errors that can occur during Sigma decoding
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigmaDecodeError {
    InvalidHex,
    EmptyInput,
    InvalidTypeTag { expected: u8, found: u8 },
    InvalidLength { expected: usize, found: usize },
    Overflow,
}

impl std::fmt::Display for SigmaDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHex => write!(f, "Invalid hex string"),
            Self::EmptyInput => write!(f, "Empty input"),
            Self::InvalidTypeTag { expected, found } => {
                write!(
                    f,
                    "Invalid type tag: expected 0x{:02x}, found 0x{:02x}",
                    expected, found
                )
            }
            Self::InvalidLength { expected, found } => {
                write!(
                    f,
                    "Invalid length: expected {} bytes, found {}",
                    expected, found
                )
            }
            Self::Overflow => write!(f, "Value overflow during VLQ decoding"),
        }
    }
}

impl std::error::Error for SigmaDecodeError {}

/// VLQ-encode a u64 value and append to buffer
fn vlq_encode(buf: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if n == 0 {
            break;
        }
    }
}

/// Encode a `Coll[SByte]` value as a Sigma register hex string.
///
/// Sigma serialization format:
/// ```text
/// 0e        -- type descriptor: Coll[SByte]
/// <VLQ>     -- length of byte array
/// <bytes>   -- raw bytes
/// ```
pub fn encode_sigma_coll_byte(data: &[u8]) -> String {
    let mut bytes = vec![0x0eu8]; // Coll[SByte] type code
    vlq_encode(&mut bytes, data.len() as u64);
    bytes.extend_from_slice(data);
    hex::encode(bytes)
}

/// Encode a `Coll[Coll[SByte]]` value as a Sigma register hex string.
///
/// This is used by Rosen Bridge to store metadata in R4 of lock boxes.
/// Each inner element is a byte array (e.g. chain name, address, fee as UTF-8).
///
/// Sigma serialization format:
/// ```text
/// 0e 0c     -- type descriptor: Coll[Coll[SByte]]
///            -- 0e = Coll of embedded type, 0c = Coll[SByte]
///            -- (SByte type code = 0x01, Coll prefix = 0x0c for Coll[SByte])
/// <VLQ>     -- number of inner collections
/// For each inner Coll[SByte]:
///   <VLQ>   -- length of byte array
///   <bytes> -- raw bytes
/// ```
///
/// Note: The type descriptor `0e 0c` comes from sigma-rust's encoding:
/// - `0x0e` = Coll with nested type follows
/// - `0x0c` = Coll[SByte] (the inner collection type)
pub fn encode_sigma_coll_coll_byte(values: &[&[u8]]) -> String {
    // Type descriptor for Coll[Coll[SByte]]
    let mut bytes = vec![0x0eu8, 0x0c];

    // Number of inner collections
    vlq_encode(&mut bytes, values.len() as u64);

    // Each inner Coll[SByte]: length-prefixed bytes
    for value in values {
        vlq_encode(&mut bytes, value.len() as u64);
        bytes.extend_from_slice(value);
    }

    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_sigma_long_zero() {
        let encoded = encode_sigma_long(0);
        assert_eq!(encoded, "0500"); // Type tag + zigzag(0) = 0
    }

    #[test]
    fn test_encode_sigma_long_positive() {
        let encoded = encode_sigma_long(1);
        assert_eq!(encoded, "0502"); // Type tag + zigzag(1) = 2

        let encoded = encode_sigma_long(100);
        assert_eq!(encoded, "05c801"); // Type tag + zigzag(100) = 200
    }

    #[test]
    fn test_encode_sigma_long_negative() {
        let encoded = encode_sigma_long(-1);
        assert_eq!(encoded, "0501"); // Type tag + zigzag(-1) = 1

        let encoded = encode_sigma_long(-100);
        assert_eq!(encoded, "05c701"); // Type tag + zigzag(-100) = 199
    }

    #[test]
    fn test_encode_sigma_long_large() {
        // Large value that requires multiple VLQ bytes
        let value: i64 = 1_000_000_000_000;
        let encoded = encode_sigma_long(value);
        assert!(encoded.starts_with("05"));
        assert!(encoded.len() > 4); // More than just type tag + 1 byte
    }

    #[test]
    fn test_decode_sigma_long_roundtrip() {
        let test_values = [
            0i64,
            1,
            -1,
            100,
            -100,
            1_000_000,
            -1_000_000,
            i64::MAX,
            i64::MIN,
        ];

        for value in test_values {
            let encoded = encode_sigma_long(value);
            let decoded = decode_sigma_long(&encoded).unwrap();
            assert_eq!(decoded, value, "Failed roundtrip for {}", value);
        }
    }

    #[test]
    fn test_decode_sigma_long_errors() {
        // Invalid hex
        assert!(decode_sigma_long("xyz").is_err());

        // Empty
        assert!(decode_sigma_long("").is_err());

        // Wrong type tag
        assert!(matches!(
            decode_sigma_long("0600"),
            Err(SigmaDecodeError::InvalidTypeTag {
                expected: 0x05,
                found: 0x06
            })
        ));
    }

    #[test]
    fn test_encode_sigma_int_zero() {
        let encoded = encode_sigma_int(0);
        assert_eq!(encoded, "0400");
    }

    #[test]
    fn test_encode_sigma_int_positive() {
        let encoded = encode_sigma_int(1);
        assert_eq!(encoded, "0402");

        let encoded = encode_sigma_int(100);
        assert_eq!(encoded, "04c801");
    }

    #[test]
    fn test_encode_sigma_int_negative() {
        let encoded = encode_sigma_int(-1);
        assert_eq!(encoded, "0401");
    }

    #[test]
    fn test_encode_coll_byte() {
        // 32-byte box ID
        let box_id =
            hex::decode("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                .unwrap();
        let encoded = encode_sigma_coll_byte(&box_id);
        // 0e (type) + 20 (length = 32) + 32 bytes
        assert!(encoded.starts_with("0e20"));
        assert_eq!(encoded.len(), 2 + 2 + 64); // "0e" + "20" + 64 hex chars
    }

    #[test]
    fn test_encode_coll_byte_empty() {
        let encoded = encode_sigma_coll_byte(&[]);
        assert_eq!(encoded, "0e00");
    }

    #[test]
    fn test_encode_coll_coll_byte_empty() {
        let encoded = encode_sigma_coll_coll_byte(&[]);
        // Type descriptor 0e0c + count 0
        assert_eq!(encoded, "0e0c00");
    }

    #[test]
    fn test_encode_coll_coll_byte_single() {
        let data = b"cardano";
        let encoded = encode_sigma_coll_coll_byte(&[data.as_ref()]);
        // 0e0c + count=1 + len=7 + "cardano"
        assert_eq!(
            encoded,
            format!("0e0c01{:02x}{}", 7, hex::encode(b"cardano"))
        );
    }

    #[test]
    fn test_encode_coll_coll_byte_rosen_r4() {
        // Simulate a Rosen lock box R4 with typical fields:
        // [chain, address, network_fee, bridge_fee, from_address]
        let chain = b"cardano";
        let address = b"addr1qtest";
        let network_fee = b"500000";
        let bridge_fee = b"300000";
        let from_address = b"9ftest";

        let values: Vec<&[u8]> = vec![
            chain.as_ref(),
            address.as_ref(),
            network_fee.as_ref(),
            bridge_fee.as_ref(),
            from_address.as_ref(),
        ];
        let encoded = encode_sigma_coll_coll_byte(&values);

        // Verify starts with type descriptor and count=5
        assert!(encoded.starts_with("0e0c05"));

        // Decode manually to verify structure
        let bytes = hex::decode(&encoded).unwrap();
        assert_eq!(bytes[0], 0x0e); // Coll
        assert_eq!(bytes[1], 0x0c); // Coll[SByte]
        assert_eq!(bytes[2], 0x05); // 5 elements

        // First element: "cardano" (len=7)
        assert_eq!(bytes[3], 7);
        assert_eq!(&bytes[4..11], b"cardano");

        // Second element: "addr1qtest" (len=10)
        assert_eq!(bytes[11], 10);
        assert_eq!(&bytes[12..22], b"addr1qtest");
    }

    #[test]
    fn test_encode_coll_coll_byte_empty_inner() {
        // An empty inner byte array is valid
        let empty: &[u8] = &[];
        let non_empty = b"data";
        let encoded = encode_sigma_coll_coll_byte(&[empty, non_empty.as_ref()]);

        let bytes = hex::decode(&encoded).unwrap();
        assert_eq!(bytes[0], 0x0e);
        assert_eq!(bytes[1], 0x0c);
        assert_eq!(bytes[2], 0x02); // 2 elements
        assert_eq!(bytes[3], 0x00); // first element length = 0
        assert_eq!(bytes[4], 0x04); // second element length = 4
        assert_eq!(&bytes[5..9], b"data");
    }

    #[test]
    fn test_encode_group_element() {
        // A typical compressed EC point (02 prefix)
        let mut pubkey = [0u8; 33];
        pubkey[0] = 0x02;
        pubkey[1] = 0x59;
        pubkey[2] = 0x3a;
        let encoded = encode_sigma_group_element(&pubkey);
        assert!(encoded.starts_with("07"));
        assert_eq!(encoded.len(), 68); // 2 (type tag) + 66 (33 bytes)
    }

    #[test]
    fn test_decode_group_element_roundtrip() {
        let mut pubkey = [0u8; 33];
        pubkey[0] = 0x03;
        for (i, byte) in pubkey[1..33].iter_mut().enumerate() {
            *byte = (i + 1) as u8;
        }

        let encoded = encode_sigma_group_element(&pubkey);
        let decoded = decode_sigma_group_element(&encoded).unwrap();
        assert_eq!(decoded, pubkey);
    }

    #[test]
    fn test_decode_group_element_errors() {
        // Invalid hex
        assert!(decode_sigma_group_element("xyz").is_err());

        // Empty
        assert!(decode_sigma_group_element("").is_err());

        // Wrong type tag
        assert!(matches!(
            decode_sigma_group_element("0500"),
            Err(SigmaDecodeError::InvalidTypeTag {
                expected: 0x07,
                found: 0x05
            })
        ));

        // Wrong length (too short)
        assert!(matches!(
            decode_sigma_group_element("070102"),
            Err(SigmaDecodeError::InvalidLength {
                expected: 34,
                ..
            })
        ));
    }

    #[test]
    fn test_encode_sigma_long_pair_typical() {
        // Typical Duckpools values: threshold=1250, penalty=500
        let encoded = encode_sigma_long_pair(1250, 500);
        // 0x59 + zigzag(1250)=2500 + zigzag(500)=1000
        assert!(encoded.starts_with("59"));
        // Decode manually: zigzag(1250) = 2500 = 0x09C4
        //   VLQ: 0xC4 0x13 (little-endian 7-bit groups with continuation)
        //   Actually: 2500 = 0b100111000100
        //   7-bit groups: 0b0010011 0b1000100 -> 0xC4 0x13
        // zigzag(500) = 1000 = 0x03E8
        //   VLQ: 0xE8 0x07
        assert_eq!(encoded, "59c413e807");
    }

    #[test]
    fn test_encode_sigma_long_pair_zeros() {
        let encoded = encode_sigma_long_pair(0, 0);
        // 0x59 + 0x00 + 0x00
        assert_eq!(encoded, "590000");
    }

    #[test]
    fn test_encode_sigma_long_pair_negative() {
        let encoded = encode_sigma_long_pair(-1, 1);
        // zigzag(-1) = 1, zigzag(1) = 2
        assert_eq!(encoded, "590102");
    }

    #[test]
    fn test_extract_pk_from_p2pk_ergo_tree() {
        // Construct a valid P2PK ErgoTree: 0008cd + 33-byte compressed pubkey
        let mut pk = [0x02u8; 33]; // 02 prefix, rest is 02 bytes
        pk[1] = 0x59;
        pk[2] = 0x3a;
        let ergo_tree = format!("0008cd{}", hex::encode(pk));

        let result = extract_pk_from_p2pk_ergo_tree(&ergo_tree).unwrap();
        assert_eq!(result, pk);
    }

    #[test]
    fn test_extract_pk_from_non_p2pk_ergo_tree() {
        // A script ErgoTree (not P2PK) should fail
        let result = extract_pk_from_p2pk_ergo_tree("100204a00b08cd");
        assert!(result.is_err());
    }

    #[test]
    fn test_vlq_encode_large_length() {
        // Test with a byte array longer than 127 bytes (requires multi-byte VLQ)
        let large = vec![0xABu8; 200];
        let encoded = encode_sigma_coll_coll_byte(&[large.as_ref()]);

        let bytes = hex::decode(&encoded).unwrap();
        assert_eq!(bytes[0], 0x0e);
        assert_eq!(bytes[1], 0x0c);
        assert_eq!(bytes[2], 0x01); // 1 element

        // VLQ for 200: 200 = 0xC8 -> VLQ = [0xC8, 0x01] (200 & 0x7F = 0x48 | 0x80, 200 >> 7 = 1)
        assert_eq!(bytes[3], 0xC8); // 0x48 | 0x80 = 0xC8
        assert_eq!(bytes[4], 0x01); // continuation

        // Then 200 bytes of 0xAB
        assert_eq!(bytes.len(), 5 + 200);
        assert!(bytes[5..].iter().all(|&b| b == 0xAB));
    }
}
