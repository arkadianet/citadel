//! SigmaFi contract constants, templates, and supported tokens.

use blake2::digest::{consts::U32, Digest};
use blake2::Blake2b;

/// ERG bond contract ErgoTree (hex)
pub const ERG_BOND_CONTRACT: &str =
    "100204000402d805d601b2a5730000d602e4c6a70808d603db6308a7d604c1a7d605e4c6a705089592a3e4c6a70704d19683040193c27201d0720293db63087201720393c17201720493e4c67201040ec5a7d801d606b2a5730100ea02d19683060193c27201d0720293c17201e4c6a7060593e4c67201040ec5a793c27206d0720593db63087206720393c1720672047205";

/// Token bond contract template (join with token_id)
pub const TOKEN_BOND_CONTRACT_TEMPLATE: [&str; 2] = [
    "10060400040004020580897a0e20",
    "0402d805d601b2a5730000d602e4c6a70808d603db6308a7d604c1a7d605e4c6a705089592a3e4c6a70704d19683040193c27201d0720293db63087201720393c17201720493e4c67201040ec5a7d803d606db63087201d607b27206730100d608b2a5730200ea02d19683090193c27201d0720293c172017303938c7207017304938c720702e4c6a7060593b17206730593e4c67201040ec5a793c27208d0720593db63087208720393c1720872047205",
];

/// ERG order contract (on-close maturity)
pub const ORDER_ON_CLOSE_ERG_CONTRACT: &str =
    "1012040005e80705c09a0c08cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a0205a0060601000e20eccbd70bb2ed259a3f6888c4b68bbd963ff61e2d71cdfda3c7234231e1e4b76604020400043c04100400040401010402040601010101d80bd601b2a5730000d602e4c6a70408d603e4c6a70704d604e4c6a70505d605e30008d606e67205d6077301d6087302d6097303d60a957206d801d60a7e72040683024406860272099d9c7e720706720a7e7208068602e472059d9c7e730406720a7e72080683014406860272099d9c7e7207067e7204067e720806d60b730595937306cbc27201d804d60c999aa37203e4c672010704d60db2a5730700d60eb2720a730800d60f8c720e02d1ed96830b0193e4c67201040ec5a793e4c672010508720293e4c672010605e4c6a70605e6c67201080893db63087201db6308a793c17201c1a7927203730990720c730a92720c730b93c2720dd0720293c1720d7204ed9591720f720bd801d610b2a5730c009683020193c27210d08c720e01937ec1721006720f730d957206d802d610b2720a730e00d6118c72100295917211720bd801d612b2a5730f009683020193c27212d08c721001937ec17212067211731073117202";

/// ERG order contract (fixed-height maturity)
pub const ORDER_FIXED_HEIGHT_ERG_CONTRACT: &str =
    "100f040005e80705c09a0c08cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a0205a0060601000e20eccbd70bb2ed259a3f6888c4b68bbd963ff61e2d71cdfda3c7234231e1e4b76604020400040401010402040601010101d80ad601b2a5730000d602e4c6a70408d603e4c6a70505d604e30008d605e67204d6067301d6077302d6087303d609957205d801d6097e72030683024406860272089d9c7e72060672097e7207068602e472049d9c7e73040672097e72070683014406860272089d9c7e7206067e7203067e720706d60a730595937306cbc27201d803d60bb2a5730700d60cb27209730800d60d8c720c02d1ed9683090193e4c67201040ec5a793e4c672010508720293e4c672010605e4c6a70605e6c67201080893db63087201db6308a793c17201c1a793e4c672010704e4c6a7070493c2720bd0720293c1720b7203ed9591720d720ad801d60eb2a57309009683020193c2720ed08c720c01937ec1720e06720d730a957205d802d60eb27209730b00d60f8c720e029591720f720ad801d610b2a5730c009683020193c27210d08c720e01937ec1721006720f730d730e7202";

/// Token order contract template (on-close maturity)
/// Segments: [0] + token_id + [1] + bond_contract_hash + [2]
pub const ORDER_ON_CLOSE_TOKEN_TEMPLATE: [&str; 3] = [
    "101c04000e20",
    "05e80705c09a0c08cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a0205a0060601000e20",
    "040204000400043c041004000580897a0402040404000580897a040201010402040604000580897a040201010101d80cd601b2a5730000d602e4c6a70408d603e4c6a70704d6047301d605e4c6a70505d606e30008d607e67206d6087302d6097303d60a7304d60b957207d801d60b7e720506830244068602720a9d9c7e720806720b7e7209068602e472069d9c7e730506720b7e720906830144068602720a9d9c7e7208067e7205067e720906d60c730695937307cbc27201d806d60d999aa37203e4c672010704d60eb2a5730800d60fdb6308720ed610b2720f730900d611b2720b730a00d6128c721102d1ed96830e0193e4c67201040ec5a793e4c672010508720293e4c672010605e4c6a70605e6c67201080893db63087201db6308a793c17201c1a7927203730b90720d730c92720d730d93c2720ed0720293c1720e730e938c7210017204938c721002720593b1720f730fed95917212720cd803d613b2a5731000d614db63087213d615b272147311009683050193c27213d08c72110193c172137312938c7215017204937e8c72150206721293b1721473137314957207d802d613b2720b731500d6148c72130295917214720cd803d615b2a5731600d616db63087215d617b272167317009683050193c27215d08c72130193c172157318938c7217017204937e8c72170206721493b172167319731a731b7202",
];

/// Token order contract template (fixed-height maturity)
pub const ORDER_FIXED_HEIGHT_TOKEN_TEMPLATE: [&str; 3] = [
    "101904000e20",
    "05e80705c09a0c08cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a0205a0060601000e20",
    "0402040004000580897a0402040404000580897a040201010402040604000580897a040201010101d80bd601b2a5730000d602e4c6a70408d6037301d604e4c6a70505d605e30008d606e67205d6077302d6087303d6097304d60a957206d801d60a7e72040683024406860272099d9c7e720706720a7e7208068602e472059d9c7e730506720a7e72080683014406860272099d9c7e7207067e7204067e720806d60b730695937307cbc27201d805d60cb2a5730800d60ddb6308720cd60eb2720d730900d60fb2720a730a00d6108c720f02d1ed96830c0193e4c67201040ec5a793e4c672010508720293e4c672010605e4c6a70605e6c67201080893db63087201db6308a793c17201c1a793e4c672010704e4c6a7070493c2720cd0720293c1720c730b938c720e017203938c720e02720493b1720d730ced95917210720bd803d611b2a5730d00d612db63087211d613b27212730e009683050193c27211d08c720f0193c17211730f938c7213017203937e8c72130206721093b1721273107311957206d802d611b2720a731200d6128c72110295917212720bd803d613b2a5731300d614db63087213d615b272147314009683050193c27213d08c72110193c172137315938c7215017203937e8c72150206721293b172147316731773187202",
];

/// SigmaFi developer fee address (ErgoTree hex)
pub const DEV_FEE_ERGO_TREE: &str =
    "0008cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a02";

/// Developer fee: 0.5% (500 / 100_000)
pub const DEV_FEE_NUM: u64 = 500;
/// UI fee: 0.4% (400 / 100_000)
pub const UI_FEE_NUM: u64 = 400;
/// Fee denominator
pub const FEE_DENOM: u64 = 100_000;

/// Minimum box value for token-based outputs (0.001 ERG)
pub const SAFE_MIN_BOX_VALUE: i64 = 1_000_000;

/// Storage rent period in blocks (~4 years)
pub const STORAGE_PERIOD: i32 = 1_051_200;

/// Minimum bond duration in blocks (on-close orders)
pub const MIN_MATURITY_BLOCKS: i32 = 30;

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    OnClose,
    FixedHeight,
}

/// Supported loan token definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SupportedToken {
    pub token_id: &'static str,
    pub name: &'static str,
    pub decimals: u8,
}

/// Curated list of supported loan tokens
pub const SUPPORTED_TOKENS: &[SupportedToken] = &[
    SupportedToken {
        token_id: "ERG",
        name: "ERG",
        decimals: 9,
    },
    SupportedToken {
        token_id: "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04",
        name: "SigUSD",
        decimals: 2,
    },
    SupportedToken {
        token_id: "003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0",
        name: "SigRSV",
        decimals: 0,
    },
    SupportedToken {
        token_id: "7a51950e5f548549ec1aa63ffdc38279505b11e7e803d01bcf8347e0123c88b0",
        name: "rsBTC",
        decimals: 8,
    },
    SupportedToken {
        token_id: "e023c5f382b6e96fbd878f6811aac73345489032157ad5affb84aefd4956c297",
        name: "rsADA",
        decimals: 6,
    },
    SupportedToken {
        token_id: "8b08cdd5449a9592a9e79711d7d79249d7a03c535d17efaee83e216e80a44c4b",
        name: "RSN",
        decimals: 3,
    },
    SupportedToken {
        token_id: "9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922269913e0de369d",
        name: "SPF",
        decimals: 6,
    },
];

/// Build the bond contract ErgoTree for a given loan token
pub fn build_bond_contract(token_id: &str) -> String {
    if token_id == "ERG" {
        return ERG_BOND_CONTRACT.to_string();
    }
    format!(
        "{}{}{}",
        TOKEN_BOND_CONTRACT_TEMPLATE[0], token_id, TOKEN_BOND_CONTRACT_TEMPLATE[1]
    )
}

/// Build the order contract ErgoTree for a given loan token and order type
pub fn build_order_contract(token_id: &str, order_type: OrderType) -> String {
    if token_id == "ERG" {
        return match order_type {
            OrderType::OnClose => ORDER_ON_CLOSE_ERG_CONTRACT.to_string(),
            OrderType::FixedHeight => ORDER_FIXED_HEIGHT_ERG_CONTRACT.to_string(),
        };
    }

    let bond_contract = build_bond_contract(token_id);
    let bond_bytes = hex::decode(&bond_contract).expect("valid bond contract hex");
    let bond_hash = blake2b_256(&bond_bytes);
    let bond_hash_hex = hex::encode(bond_hash);

    let template = match order_type {
        OrderType::OnClose => &ORDER_ON_CLOSE_TOKEN_TEMPLATE,
        OrderType::FixedHeight => &ORDER_FIXED_HEIGHT_TOKEN_TEMPLATE,
    };

    format!(
        "{}{}{}{}{}",
        template[0], token_id, template[1], bond_hash_hex, template[2]
    )
}

/// Extract the loan token ID from an order contract's ErgoTree
pub fn extract_token_id_from_order(ergo_tree: &str) -> String {
    if ergo_tree.starts_with(ORDER_ON_CLOSE_TOKEN_TEMPLATE[0])
        || ergo_tree.starts_with(ORDER_FIXED_HEIGHT_TOKEN_TEMPLATE[0])
    {
        let start = ORDER_ON_CLOSE_TOKEN_TEMPLATE[0].len();
        if ergo_tree.len() >= start + 64 {
            return ergo_tree[start..start + 64].to_string();
        }
    }
    "ERG".to_string()
}

/// Extract the loan token ID from a bond contract's ErgoTree
pub fn extract_token_id_from_bond(ergo_tree: &str) -> String {
    if ergo_tree.starts_with(TOKEN_BOND_CONTRACT_TEMPLATE[0]) {
        let start = TOKEN_BOND_CONTRACT_TEMPLATE[0].len();
        if ergo_tree.len() >= start + 64 {
            return ergo_tree[start..start + 64].to_string();
        }
    }
    "ERG".to_string()
}

/// Blake2b-256 hash
fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_erg_bond_contract() {
        assert_eq!(build_bond_contract("ERG"), ERG_BOND_CONTRACT);
    }

    #[test]
    fn test_build_token_bond_contract() {
        let token_id = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";
        let contract = build_bond_contract(token_id);
        assert!(contract.starts_with(TOKEN_BOND_CONTRACT_TEMPLATE[0]));
        assert!(contract.contains(token_id));
    }

    #[test]
    fn test_build_erg_order_contract() {
        let on_close = build_order_contract("ERG", OrderType::OnClose);
        assert_eq!(on_close, ORDER_ON_CLOSE_ERG_CONTRACT);

        let fixed = build_order_contract("ERG", OrderType::FixedHeight);
        assert_eq!(fixed, ORDER_FIXED_HEIGHT_ERG_CONTRACT);
    }

    #[test]
    fn test_build_token_order_contract() {
        let token_id = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";
        let contract = build_order_contract(token_id, OrderType::OnClose);
        assert!(contract.starts_with(ORDER_ON_CLOSE_TOKEN_TEMPLATE[0]));
        assert!(contract.contains(token_id));
    }

    #[test]
    fn test_extract_token_id_from_erg_order() {
        assert_eq!(
            extract_token_id_from_order(ORDER_ON_CLOSE_ERG_CONTRACT),
            "ERG"
        );
    }

    #[test]
    fn test_extract_token_id_from_token_order() {
        let token_id = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";
        let contract = build_order_contract(token_id, OrderType::OnClose);
        assert_eq!(extract_token_id_from_order(&contract), token_id);
    }

    #[test]
    fn test_extract_token_id_from_erg_bond() {
        assert_eq!(extract_token_id_from_bond(ERG_BOND_CONTRACT), "ERG");
    }

    #[test]
    fn test_extract_token_id_from_token_bond() {
        let token_id = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";
        let contract = build_bond_contract(token_id);
        assert_eq!(extract_token_id_from_bond(&contract), token_id);
    }
}
