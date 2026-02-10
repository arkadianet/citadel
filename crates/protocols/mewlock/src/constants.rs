//! MewLock contract constants and fee parameters

/// MewLock contract ErgoTree hex (constant-segregated)
pub const MEWLOCK_ERGO_TREE: &str = "19a3030e05a01f05a01f05c09a0c040008cd02593abf7a55bd30ecb0d9cc89284f577db9c673bd6dba3642d5ec2eba1b131a02040202010400040204000400020204000100d80ed601e4e30002d602e4c6a70406d603e4c6a70605d60495917203730072037301d6057302d6069d9c72027e9ae4c6a709057204067e720506d607e4c6a70508d608e4c6a7070ed60993b172087303d60a9d9c72027e7204067e720506d60b7304d60cc2a7d60d93b1b5a4d9010d6393c2720d720c7305d60eafa5d9010e6394c2720e720c959372017306d802d60fb2a5730700d610b2a5730800d19683040195720996830201927ec1720f06997202720693c2720fd07207d801d611b2db6308720f73090096830301938c7211017208927e8c72110206997202720693c2720fd0720795720996830201927ec1721006720a93c27210d0720bd801d611b2db63087210730a009683030193c27210d0720b938c7211017208927e8c72110206720a720d720e95937201730bd801d60fb2a5730c00ea02d1968303019683030193c1720fc1a793c2720fd0720793db6308720fdb6308a7720d720e7207d1730d";

/// Dev treasury address
pub const DEV_ADDRESS: &str = "9fCMmB72WcFLseNx6QANheTCrDjKeb9FzdFNTdBREt2FzHTmusY";

/// Fee numerator: 3% = 3000/100000
pub const FEE_NUM: u64 = 3000;

/// Fee denominator
pub const FEE_DENOM: u64 = 100000;

/// Minimum ERG value (nanoERG) to charge a fee on
/// From contract: `SELF.value > 100000L`
pub const MIN_ERG_FOR_FEE: u64 = 100_000;

/// Minimum token quantity to charge a fee on
/// From contract: quantity > 34
pub const MIN_TOKEN_FOR_FEE: u64 = 34;

/// Duration presets: (label, blocks)
/// Ergo block time ~2 minutes
pub const DURATION_PRESETS: &[(&str, i32)] = &[
    ("1 Month", 21_600),
    ("3 Months", 64_800),
    ("6 Months", 129_600),
    ("1 Year", 259_200),
    ("2 Years", 518_400),
];

/// Calculate the fee for an ERG amount
/// Returns 0 if below threshold
pub fn calculate_erg_fee(erg_value: u64) -> u64 {
    if erg_value <= MIN_ERG_FOR_FEE {
        return 0;
    }
    let fee = (erg_value * FEE_NUM) / FEE_DENOM;
    let max_fee = erg_value / 10; // Cap at 10%
    fee.min(max_fee)
}

/// Calculate the fee for a token amount
/// Returns 0 if below threshold
pub fn calculate_token_fee(amount: u64) -> u64 {
    if amount <= MIN_TOKEN_FOR_FEE {
        return 0;
    }
    let fee = (amount * FEE_NUM) / FEE_DENOM;
    let max_fee = amount / 10; // Cap at 10%
    fee.min(max_fee)
}

/// Expected MewLock contract P2S address (mainnet).
/// This is the address form of MEWLOCK_ERGO_TREE.
pub const MEWLOCK_ADDRESS: &str = "5adWKCNFaCzfHxRxzoFvAS7khVsqXqvKV6cejDimUXDUWJNJFhRaTmT65PRUPv2fGeXJQ2Yp9GqpiQayHqMRkySDMnWW7X3tBsjgwgT11pa1NuJ3cxf4Xvxo81Vt4HmY3KCxkg1aptVZdCSDA7ASiYE6hRgN5XnyPsaAY2Xc7FUoWN1ndQRA7Km7rjcxr3NHFPirZvTbZfB298EYwDfEvrZmSZhU2FGpMUbmVpdQSbooh8dGMjCf4mXrP2N4FSkDaNVZZPcEPyDr4WM1WHrVtNAEAoWJUTXQKeLEj6srAsPw7PpXgKa74n3Xc7qiXEr2Tut7jJkFLeNqLouQN13kRwyyADQ5aXTCBuhqsucQvyqEEEk7ekPRnqk4LzRyVqCVsRZ7Y5Kk1r1jZjPeXSUCTQGnL1pdFfuJ1SfaYkbgebjnJT2KJWVRamQjztvrhwarcVHDXbUKNawznfJtPVm7abUv81mro23AKhhkPXkAweZ4jXdKwQxjiAqCCBNBMNDXk66AhdKCbK5jFqnZWPwKm6eZ1BXjr9Au8sjhi4HKhrxZWbvr4yi9bBFFKbzhhQm9dVcMpCB3S5Yj2m6XaHaivHN1DFCPBo6nQRV9sBMYZrP3tbCtgKgiTLZWLNNPLFPWhmoR1DABBGnVe5GYNwTxJZY2Mc2u8KZQC4pLqkHJmdq2hHSfaxzK77QXtzyyk59z4EBjyMWeVCtrcDg2jZBepPhoT6i5xUAkzBzhGK3SFor2v44yahHZiHNPj5W3LEU9mFCdiPwNCVd9S2a5MNZJHBukWKVjVF4s5bhXkCzW2MbXjAH1cue4APHYvobkPpn2zd9vnwLow8abjAdLBmTz2idAWchsavdU";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_erg_fee_below_threshold() {
        assert_eq!(calculate_erg_fee(100_000), 0);
        assert_eq!(calculate_erg_fee(50_000), 0);
        assert_eq!(calculate_erg_fee(0), 0);
    }

    #[test]
    fn test_erg_fee_3_percent() {
        // 1 ERG = 1_000_000_000 nanoERG
        let fee = calculate_erg_fee(1_000_000_000);
        assert_eq!(fee, 30_000_000); // 3% of 1 ERG
    }

    #[test]
    fn test_erg_fee_cap_at_10_percent() {
        // FEE_NUM/FEE_DENOM = 3% which is always < 10%, so cap never triggers
        // with the current 3% rate
        let fee = calculate_erg_fee(10_000_000_000);
        assert_eq!(fee, 300_000_000); // 3%
        assert!(fee <= 10_000_000_000 / 10); // <= 10%
    }

    #[test]
    fn test_token_fee_below_threshold() {
        assert_eq!(calculate_token_fee(34), 0);
        assert_eq!(calculate_token_fee(10), 0);
        assert_eq!(calculate_token_fee(0), 0);
    }

    #[test]
    fn test_token_fee_3_percent() {
        let fee = calculate_token_fee(1000);
        assert_eq!(fee, 30); // 3% of 1000
    }

    #[test]
    fn test_token_fee_small_amount() {
        // 35 tokens: just above threshold
        let fee = calculate_token_fee(35);
        assert_eq!(fee, 1); // (35 * 3000) / 100000 = 1.05 → 1
    }

    /// Verify that Address::recreate_from_ergo_tree does NOT produce the
    /// correct P2S address for constant-segregated ErgoTrees. This is why we
    /// hardcode MEWLOCK_ADDRESS instead of deriving it from MEWLOCK_ERGO_TREE.
    #[test]
    fn test_constant_segregated_tree_does_not_roundtrip() {
        use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
        use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

        let tree_bytes = hex::decode(MEWLOCK_ERGO_TREE).unwrap();
        let tree = ErgoTree::sigma_parse_bytes(&tree_bytes).unwrap();
        let address = Address::recreate_from_ergo_tree(&tree).unwrap();
        let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
        let addr_str = encoder.address_to_str(&address);

        // The derived address differs because sigma-rust strips constants
        // from the segregated tree. This confirms we must use the hardcoded
        // MEWLOCK_ADDRESS for box queries.
        assert_ne!(addr_str, MEWLOCK_ADDRESS);
        assert!(MEWLOCK_ADDRESS.starts_with("5ad"));
    }
}
