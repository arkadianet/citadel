//! HodlCoin Constants
//!
//! Bank contract ErgoTree, token layout, and fee parameters.

/// hodlERG bank contract ErgoTree (hex)
pub const HODLERG_BANK_ERGO_TREE: &str = "100a040204000402040004000502050005d00f04040e2002a195c991b685f1bbf6c84cb192f698ecccc3c707b7293c21d27655ade8d56ed812d601db6308a7d602b27201730000d6038c720202d604b2a5730100d605db63087204d606b27205730200d6078c720602d6089972037207d609c17204d60a7ec1a706d60be4c6a70505d60c7e720b06d60de4c6a70405d60e9d9c720a720c7e99720d720306d60fe4c6a70605d610e4c6a70705d611e4c6a70805d61296830401927209720f93c27204c2a79683030193b27205730300b27201730400938c7206018c72020192720773059683050193e4c672040405720d93e4c672040505720b93e4c672040605720f93e4c672040705721093e4c6720408057211959172087306d1968302017212927e7209069a720a9d9c7e720806720e720cd803d6139d9c7e997207720306720e720cd6147307d615b2a5730800d1968303017212937e7209069a99720a72139d9c72137e7211067e72140696830201937ec17215069d9c72137e7210067e72140693cbc272157309";

/// Fee denominator (all fee numerators are out of 1000)
pub const FEE_DENOM: i64 = 1000;

/// Minimum miner fee in nanoERG (0.0011 ERG)
pub const MIN_MINER_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Minimum box value in nanoERG
pub const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

/// Minimum change box value
pub const MIN_CHANGE_VALUE: u64 = 1_000_000;

/// Minimum TVL (nanoERG) to display a bank (filters out test/abandoned deployments)
pub const MIN_DISPLAY_TVL: i64 = 10_000_000_000; // 10 ERG

/// Bank token layout
pub mod bank_tokens {
    /// Singleton NFT at index 0
    pub const SINGLETON: usize = 0;
    /// hodlToken at index 1
    pub const HODL_TOKEN: usize = 1;
}

/// Bank register layout
pub mod bank_registers {
    /// R4 = Total token supply (Long)
    pub const R4_TOTAL_SUPPLY: u8 = 4;
    /// R5 = Precision factor (Long)
    pub const R5_PRECISION: u8 = 5;
    /// R6 = Minimum bank nanoERG value (Long)
    pub const R6_MIN_BANK_VALUE: u8 = 6;
    /// R7 = Dev fee numerator (Long)
    pub const R7_DEV_FEE: u8 = 7;
    /// R8 = Bank fee numerator (Long)
    pub const R8_BANK_FEE: u8 = 8;
}

/// blake2b256 hash of the dev fee contract ErgoTree (embedded in the bank contract).
/// The bank script checks: blake2b256(OUTPUTS(2).propBytes) == this hash.
pub const DEV_FEE_CONTRACT_HASH: &str =
    "02a195c991b685f1bbf6c84cb192f698ecccc3c707b7293c21d27655ade8d56e";

/// Dev fee contract ErgoTree (hex) for hodlERG burns.
/// This is the compiled `phoenix_v1_hodlcoin_fee.es` contract (with $minerFee = 1100000).
/// blake2b256 of these bytes == DEV_FEE_CONTRACT_HASH.
pub const DEV_FEE_CONTRACT_BYTES: &str = "101705000400040a053205c80108cd0329bd895314c80845841b988371bed38942748983eec1da61358b5fa848f8d1a3040208cd036cfe5ecd80b5ccc6b130aed8f526705b48f770e87f7c9bd6fb393fcdadb7ace4040408cd03fe709b7fb79ad097c234e42d2218ba6873239e5cb177b91e1524712ddc26e883040608cd03e8196967038a183915bd79c249385904a9264cf81183099d80254e6c0166d3a6040808cd02d3f408925bfaec210be688bd0893de168130370386be4bb48d2f5f08c51a098e0580ade204051e05c801051405c8010580897a0e20e540cceffd3b8dd0f401193576cc413467039695969427df94454193dddfb375040c0402d80fd601b0ada5d9010163c172017300d90101599a8c7201018c720102d602b2a5730100d603b2a5730200d604c17203d6059972017204d6069d9c730372057304d6077305d608b2a5730600d6097307d60ab2a5730800d60b7309d60cb2a5730a00d60d730bd60eb2a5730c00d60f730dea02d196830401927201730e96830501ed93c17202720693c27202d07207ed93c17208720693c27208d07209ed93c1720a720693c2720ad0720bed93c1720c9d9c730f7205731093c2720cd0720ded93c1720e9d9c73117205731293c2720ed0720f96830201927204731393cbc27203731493b1a5731598731683050872077209720b720d720f";
