//! MewLock contract constants and fee parameters

/// MewLock Timelock contract ErgoTree hex (constant-segregated).
///
/// IMPORTANT: The reference implementation's `const.ts` has a DIFFERENT ErgoTree
/// (`19a303...`) which is the Mew Finance Marketplace contract, NOT the timelock.
/// The correct timelock ErgoTree is from the boxes at MEWLOCK_ADDRESS (`5adWKCN...`).
pub const MEWLOCK_ERGO_TREE: &str = "19f1042105c09a0c05f02e05c09a0c050005140500050005000544051405feffffffffffffffff0105f02e05c09a0c0500050005c80105000500050004000400050005000402040008cd02593abf7a55bd30ecb0d9cc89284f577db9c673bd6dba3642d5ec2eba1b131a020101040001010101010104000101d808d601c2a7d602c1a7d603959172027300d801d6039d9c72027301730295ed91720373039072039d72027304720373057306d6049172037307d605db6308a7d606b5ad7205d901064d0ed801d6088c72060286028c720601959172087308d801d6099d7208730995907208730ad801d60a9d9c7208730b730c95ed91720a730d90720a7209720a730ed801d60a9d7208730f95ed91720a731090720a7209720a72097311d901064d0e918c7206027312d607cde4c6a70407d608b5ad7205d901084d0ed802d60a8c720801d60bb57206d9010b4d0e938c720b01720a8602720a998c7208029591b1720b73138cb2720b731400027315d901084d0e918c7208027316ea02d19683040192a3e4c6a7050493b1b5a4d901096393c2720972017317afa5d901096394c272097201eded95ec720491b172067318aea5d9010963eded93c27209d0731995720492c172097203731a9591b17206731baf7206d9010b4d0eaedb63087209d9010d4d0eed938c720d018c720b01928c720d028c720b02731c731ded9572049272027203731eaf7206d901094d0eae7205d9010b4d0eed938c720b018c720901928c720b028c720902aea5d9010963eded93c27209d0720792c1720999720272039591b17208731faf7208d9010b4d0eaedb63087209d9010d4d0eed938c720d018c720b01928c720d028c720b0273207207";

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

/// MewLock Timelock contract P2S address (mainnet).
/// Verified: boxes at this address have ErgoTree matching MEWLOCK_ERGO_TREE
/// with registers R4=GroupElement, R5=Int, R6=Int (matching our layout).
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

    /// Verify that the ErgoTree is a valid constant-segregated tree that
    /// sigma-rust can parse, and that bytes roundtrip correctly.
    #[test]
    fn test_ergo_tree_parses_and_roundtrips() {
        use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

        let original_bytes = hex::decode(MEWLOCK_ERGO_TREE).unwrap();
        assert_eq!(
            original_bytes[0] & 0x01,
            1,
            "ErgoTree should have constant segregation flag set"
        );

        let tree = ErgoTree::sigma_parse_bytes(&original_bytes).unwrap();
        let serialized_bytes = tree.sigma_serialize_bytes().unwrap();
        assert_eq!(
            original_bytes, serialized_bytes,
            "ErgoTree bytes must roundtrip exactly through sigma-rust"
        );
    }

    /// Verify MEWLOCK_ADDRESS starts with expected prefix.
    /// NOTE: Address::recreate_from_ergo_tree() does NOT produce the correct
    /// P2S address for constant-segregated ErgoTrees — it strips constants.
    /// We hardcode MEWLOCK_ADDRESS instead of deriving it.
    #[test]
    fn test_mewlock_address_prefix() {
        assert!(MEWLOCK_ADDRESS.starts_with("5adWKCN"));
    }
}
