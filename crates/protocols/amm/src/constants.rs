//! AMM Constants
//!
//! Pool contract templates and fee parameters for Spectrum DEX pools.

/// Pool Contract Templates (ErgoTree Hex)
pub mod pool_templates {
    /// N2T (Native-to-Token) Pool Contract Template
    pub const N2T_POOL_TEMPLATE: &str = "1999030f0400040204020404040405feffffffffffffffff0105feffffffffffffffff01050004d00f040004000406050005000580dac409d819d601b2a5730000d602e4c6a70404d603db63087201d604db6308a7d605b27203730100d606b27204730200d607b27203730300d608b27204730400d6099973058c720602d60a999973068c7205027209d60bc17201d60cc1a7d60d99720b720cd60e91720d7307d60f8c720802d6107e720f06d6117e720d06d612998c720702720fd6137e720c06d6147308d6157e721206d6167e720a06d6177e720906d6189c72117217d6199c72157217d1ededededededed93c27201c2a793e4c672010404720293b27203730900b27204730a00938c7205018c720601938c7207018c72080193b17203730b9593720a730c95720e929c9c721072117e7202069c7ef07212069a9c72137e7214067e9c720d7e72020506929c9c721372157e7202069c7ef0720d069a9c72107e7214067e9c72127e7202050695ed720e917212730d907216a19d721872139d72197210ed9272189c721672139272199c7216721091720b730e";

    /// T2T (Token-to-Token) Pool Contract Template
    pub const T2T_POOL_TEMPLATE: &str = "19a9030f040004020402040404040406040605feffffffffffffffff0105feffffffffffffffff01050004d00f0400040005000500d81ad601b2a5730000d602e4c6a70404d603db63087201d604db6308a7d605b27203730100d606b27204730200d607b27203730300d608b27204730400d609b27203730500d60ab27204730600d60b9973078c720602d60c999973088c720502720bd60d8c720802d60e998c720702720dd60f91720e7309d6108c720a02d6117e721006d6127e720e06d613998c7209027210d6147e720d06d615730ad6167e721306d6177e720c06d6187e720b06d6199c72127218d61a9c72167218d1edededededed93c27201c2a793e4c672010404720292c17201c1a793b27203730b00b27204730c00938c7205018c720601ed938c7207018c720801938c7209018c720a019593720c730d95720f929c9c721172127e7202069c7ef07213069a9c72147e7215067e9c720e7e72020506929c9c721472167e7202069c7ef0720e069a9c72117e7215067e9c72137e7202050695ed720f917213730e907217a19d721972149d721a7211ed9272199c7217721492721a9c72177211";
}

/// Token indices in pool boxes
pub mod pool_indices {
    /// N2T Pool Token Layout
    pub mod n2t {
        /// Pool NFT (unique identifier) at index 0
        pub const INDEX_NFT: usize = 0;
        /// LP tokens (locked) at index 1
        pub const INDEX_LP: usize = 1;
        /// Token Y (non-native asset) at index 2
        pub const INDEX_Y: usize = 2;
        // Note: Token X (ERG) is in box value, not in tokens array
    }

    /// T2T Pool Token Layout
    pub mod t2t {
        /// Pool NFT (unique identifier) at index 0
        pub const INDEX_NFT: usize = 0;
        /// LP tokens (locked) at index 1
        pub const INDEX_LP: usize = 1;
        /// Token X at index 2
        pub const INDEX_X: usize = 2;
        /// Token Y at index 3
        pub const INDEX_Y: usize = 3;
    }
}

/// Fee constants
pub mod fees {
    /// Default fee numerator (0.3% fee = 997/1000)
    pub const DEFAULT_FEE_NUM: i32 = 997;

    /// Default fee denominator
    pub const DEFAULT_FEE_DENOM: i32 = 1000;
}

/// LP token constants
pub mod lp {
    /// Total LP token emission (max i64 value)
    pub const TOTAL_EMISSION: i64 = 0x7fffffffffffffff; // 9,223,372,036,854,775,807
}

/// Swap order contract templates (ErgoTree hex)
///
/// These are Spectrum V3 swap contracts. Constants are substituted at specific
/// positions to create order-specific contracts. The contracts validate that
/// off-chain bots execute swaps at fair prices.
///
/// Key constants embedded via template substitution:
/// - PoolNFT: Target pool ID
/// - RedeemerPropBytes: User's address (where output goes)
/// - MinQuoteAmount: Minimum acceptable output (slippage protection)
/// - BaseAmount: Amount being swapped
/// - FeeNum/FeeDenom: Pool fee parameters
/// - MaxExFee: Maximum execution fee for bot
/// - MaxMinerFee: Maximum miner fee
/// - RefundProp: User's public key (for refund if order not filled)
pub mod swap_templates {
    /// N2T SwapSell: User sends ERG, receives Token
    /// Spectrum SPF fee N2T SwapSell contract - full ErgoTree with segregated constants
    /// and default placeholder values. Use `ErgoTree::with_constant()` to substitute.
    ///
    /// Constant positions:
    /// {1}=ExFeePerTokenDenom[Long], {2}=Delta[Long], {3}=BaseAmount[Long],
    /// {4}=FeeNum[Int], {5}=RefundProp[ProveDlog], {10}=SpectrumIsQuote[Boolean],
    /// {11}=MaxExFee[Long], {13}=PoolNFT[Coll[Byte]], {14}=RedeemerPropBytes[Coll[Byte]],
    /// {15}=QuoteId[Coll[Byte]], {16}=MinQuoteAmount[Long],
    /// {23}=SpectrumId[Coll[Byte]], {27}=FeeDenom[Int],
    /// {28}=MinerPropBytes[Coll[Byte]], {31}=MaxMinerFee[Long]
    pub const N2T_SWAP_SELL_TEMPLATE: &str = "19fe04210400059cdb0205cead0105e01204c80f08cd02217daf90deb73bdf8b6709bb42093fdfaff6573fd47b630e2d3fdd4a8193a74d0404040604020400010105f01504000e2002020202020202020202020202020202020202020202020202020202020202020e2001010101010101010101010101010101010101010101010101010101010101010e20040404040404040404040404040404040404040404040404040404040404040405c00c0101010105f015060100040404020e2003030303030303030303030303030303030303030303030303030303030303030101040406010104d00f0e691005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a573040500050005a09c010100d804d601b2a4730000d6027301d6037302d6049c73037e730405eb027305d195ed92b1a4730693b1db630872017307d806d605db63087201d606b2a5730800d607db63087206d608b27207730900d6098c720802d60a95730a9d9c7e997209730b067e7202067e7203067e720906edededededed938cb27205730c0001730d93c27206730e938c720801730f92720a7e7310069573117312d801d60b997e7313069d9c720a7e7203067e72020695ed91720b731492b172077315d801d60cb27207731600ed938c720c017317927e8c720c0206720b7318909c7e8cb2720573190002067e7204069c9a720a731a9a9c7ec17201067e731b067e72040690b0ada5d9010b639593c2720b731cc1720b731d731ed9010b599a8c720b018c720b02731f7320";

    /// N2T SwapBuy: User sends Token, receives ERG
    /// Spectrum SPF fee N2T SwapBuy contract - full ErgoTree with segregated constants
    /// and default placeholder values. Use `ErgoTree::with_constant()` to substitute.
    ///
    /// Constant positions:
    /// {1}=BaseAmount[Long], {2}=FeeNum[Int], {3}=RefundProp[ProveDlog],
    /// {7}=MaxExFee[Long], {8}=ExFeePerTokenDenom[Long], {9}=ExFeePerTokenNum[Long],
    /// {11}=PoolNFT[Coll[Byte]], {12}=RedeemerPropBytes[Coll[Byte]],
    /// {13}=MinQuoteAmount[Long], {16}=SpectrumId[Coll[Byte]],
    /// {20}=FeeDenom[Int], {21}=MinerPropBytes[Coll[Byte]], {24}=MaxMinerFee[Long]
    pub const N2T_SWAP_BUY_TEMPLATE: &str = "198b041a040005e01204c80f08cd02217daf90deb73bdf8b6709bb42093fdfaff6573fd47b630e2d3fdd4a8193a74d04040406040205f015052c05c80104000e2002020202020202020202020202020202020202020202020202020202020202020e20010101010101010101010101010101010101010101010101010101010101010105c00c06010004000e20030303030303030303030303030303030303030303030303030303030303030301010502040404d00f0e691005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a573040500050005a09c010100d802d601b2a4730000d6029c73017e730205eb027303d195ed92b1a4730493b1db630872017305d804d603db63087201d604b2a5730600d60599c17204c1a7d606997e7307069d9c7e7205067e7308067e730906ededededed938cb27203730a0001730b93c27204730c927205730d95917206730ed801d607b2db63087204730f00ed938c7207017310927e8c7207020672067311909c7ec17201067e7202069c7e9a72057312069a9c7e8cb2720373130002067e7314067e72020690b0ada5d90107639593c272077315c1720773167317d90107599a8c7207018c72070273187319";

    /// T2T Swap: User sends Token X, receives Token Y (or vice versa)
    /// From Spectrum V3 t2t Swap contract (raw expression body - not yet
    /// wrapped as full ErgoTree with constants segregation)
    pub const T2T_SWAP_TEMPLATE: &str = "d804d601b2a4730000d6027301d6037302d6049c73037e730405eb027305d195ed92b1a4730693b1db630872017307d806d605db63087201d606b2a5730800d607db63087206d608b27207730900d6098c720802d60a95730a9d9c7e997209730b067e7202067e7203067e720906edededededed938cb27205730c0001730d93c27206730e938c720801730f92720a7e7310069573117312d801d60b997e7313069d9c720a7e7203067e72020695ed91720b731492b172077315d801d60cb27207731600ed938c720c017317927e8c720c0206720b7318909c7e8cb2720573190002067e7204069c9a720a731a9a9c7ec17201067e731b067e72040690b0ada5d9010b639593c2720b731cc1720b731d731ed9010b599a8c720b018c720b02731f7320";

    /// Miner fee proposition bytes (standard P2PK miner address)
    /// Re-exported from citadel_core for convenience within swap template usage
    pub const MINER_FEE_ERGO_TREE: &str = citadel_core::constants::MINER_FEE_ERGO_TREE;

    /// SPF token ID (Spectrum Finance token for execution fees)
    pub const SPF_TOKEN_ID: &str =
        "9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922f63820814b4000";

    /// Default max execution fee (nanoERG) - 2 ERG
    pub const DEFAULT_MAX_EX_FEE: u64 = 2_000_000_000;

    /// Default max miner fee (nanoERG) - 0.005 ERG
    pub const DEFAULT_MAX_MINER_FEE: u64 = 5_000_000;

    /// Execution fee per token numerator default
    pub const DEFAULT_EX_FEE_PER_TOKEN_NUM: u64 = 1;

    /// Execution fee per token denominator default
    pub const DEFAULT_EX_FEE_PER_TOKEN_DENOM: u64 = 1;
}

/// ERG constants
pub mod erg {
    /// 1 ERG in nanoERG
    pub const NANOERG_PER_ERG: u64 = 1_000_000_000;

    /// ERG decimal places
    pub const DECIMALS: u8 = 9;

    /// Minimum storage rent in nanoERG (0.01 ERG)
    pub const MIN_STORAGE_RENT: u64 = 10_000_000;
}

/// Pre-computed ErgoTree template bytes for pool contract matching.
pub mod pool_template_bytes {
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use std::sync::LazyLock;

    fn compute_template(hex_str: &str) -> Vec<u8> {
        let bytes = hex::decode(hex_str).expect("invalid template hex");
        let tree = ErgoTree::sigma_parse_bytes(&bytes).expect("failed to parse ErgoTree");
        tree.template_bytes().expect("failed to get template bytes")
    }

    pub static N2T_POOL: LazyLock<Vec<u8>> =
        LazyLock::new(|| compute_template(super::pool_templates::N2T_POOL_TEMPLATE));
    pub static T2T_POOL: LazyLock<Vec<u8>> =
        LazyLock::new(|| compute_template(super::pool_templates::T2T_POOL_TEMPLATE));
}

/// Pre-computed ErgoTree template bytes for swap order matching.
pub mod swap_template_bytes {
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use std::sync::LazyLock;

    fn compute_template(hex_str: &str) -> Vec<u8> {
        let bytes = hex::decode(hex_str).expect("invalid template hex");
        let tree = ErgoTree::sigma_parse_bytes(&bytes).expect("failed to parse ErgoTree");
        tree.template_bytes().expect("failed to get template bytes")
    }

    pub static N2T_SWAP_SELL: LazyLock<Vec<u8>> =
        LazyLock::new(|| compute_template(super::swap_templates::N2T_SWAP_SELL_TEMPLATE));
    pub static N2T_SWAP_BUY: LazyLock<Vec<u8>> =
        LazyLock::new(|| compute_template(super::swap_templates::N2T_SWAP_BUY_TEMPLATE));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_contract_templates_are_valid_hex() {
        let templates = [
            swap_templates::N2T_SWAP_SELL_TEMPLATE,
            swap_templates::N2T_SWAP_BUY_TEMPLATE,
            swap_templates::T2T_SWAP_TEMPLATE,
        ];
        for template in &templates {
            assert!(!template.is_empty(), "Template should not be empty");
            assert!(
                hex::decode(template).is_ok(),
                "Template should be valid hex"
            );
        }
    }

    #[test]
    fn test_n2t_swap_sell_template_bytes_are_stable() {
        let bytes = &*super::swap_template_bytes::N2T_SWAP_SELL;
        assert!(!bytes.is_empty(), "Template bytes should not be empty");
        let bytes2 = &*super::swap_template_bytes::N2T_SWAP_SELL;
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn test_n2t_swap_buy_template_bytes_are_stable() {
        let bytes = &*super::swap_template_bytes::N2T_SWAP_BUY;
        assert!(!bytes.is_empty(), "Template bytes should not be empty");
        let bytes2 = &*super::swap_template_bytes::N2T_SWAP_BUY;
        assert_eq!(bytes, bytes2);
    }
}
