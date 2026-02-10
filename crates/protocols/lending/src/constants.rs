//! Duckpools Lending Protocol Constants
//!
//! Pool configurations for all Duckpools lending markets.
//! All pools defined as configuration data for easy extension.

/// Proxy contract addresses for Duckpools operations
#[derive(Debug, Clone)]
pub struct ProxyContracts {
    pub lend_address: &'static str,
    pub withdraw_address: &'static str,
    pub borrow_address: &'static str,
    pub repay_address: &'static str,
    pub partial_repay_address: &'static str,
}

/// Market configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub symbol: &'static str,
    pub pool_nft: &'static str,
    pub currency_id: Option<&'static str>,
    pub lend_token_id: &'static str,
    pub is_erg_pool: bool,
    /// Asset decimals (e.g., 9 for ERG/nanoERG, 2 for SigUSD cents, 0 for SigRSV)
    pub decimals: u8,
    pub child_nft: &'static str,
    pub parent_nft: &'static str,
    pub parameter_nft: &'static str,
    /// Borrow token ID (pool box tokens[2]). Used to find collateral boxes on-chain.
    /// Empty string if borrowing not supported for this pool.
    pub borrow_token_id: &'static str,
    pub collateral_address: &'static str,
    pub repayment_address: &'static str,
    pub proxy_contracts: ProxyContracts,
    /// Liquidation threshold for ERG collateral (e.g. 1250 = 125% LTV). 0 = no borrowing.
    pub liquidation_threshold: u64,
    /// DEX pool NFT used for price discovery of this token against ERG. None for ERG pool.
    pub collateral_dex_nft: Option<&'static str>,
}

/// Interest rate calculation constants
pub mod interest {
    pub const INTEREST_MULTIPLIER: u64 = 100_000_000;
    pub const BLOCKS_PER_YEAR: u64 = 262_800;
    pub const UPDATE_FREQUENCY_BLOCKS: u32 = 120;
}

/// Token supply constants
pub mod supply {
    pub const MAX_LEND_TOKENS_ERG: u64 = 9_000_000_001_000_000;
    pub const MAX_LEND_TOKENS_TOKEN: u64 = 9_000_000_000_000_010;
    pub const MAX_BORROW_TOKENS: u64 = 9_000_000_000_000_000;

    pub fn max_lend_tokens(is_erg_pool: bool) -> u64 {
        if is_erg_pool {
            MAX_LEND_TOKENS_ERG
        } else {
            MAX_LEND_TOKENS_TOKEN
        }
    }
}

/// Fee configuration
pub mod fees {
    pub const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;
    /// Duckpools bot uses 1M nanoERG miner fee (differs from the standard 1.1M)
    pub const DUCKPOOLS_MINER_FEE: u64 = 1_000_000;
    /// TX_FEE kept as alias for DUCKPOOLS_MINER_FEE for backward compatibility in calculator
    pub const TX_FEE: u64 = DUCKPOOLS_MINER_FEE;
    pub const MIN_LOAN_VALUE: u64 = 50_000_000;
    pub const REFUND_HEIGHT_OFFSET: i32 = 720;

    pub const ERG_THRESHOLDS: (u64, u64) = (20_000_000_000, 200_000_000_000);
    /// Token fee thresholds (in token base units, not ERG)
    /// These are lower than ERG_THRESHOLDS because they're measured in different units
    pub const TOKEN_THRESHOLDS: (u64, u64) = (2000, 200_000);

    pub const DIVISOR_ONE: u64 = 160;
    pub const DIVISOR_TWO: u64 = 200;
    pub const DIVISOR_THREE: u64 = 250;

    /// Calculate service fee matching Duckpools platform_functions.py
    pub fn calculate_service_fee(amount: u64, thresholds: (u64, u64)) -> u64 {
        let (step_one, step_two) = thresholds;

        if amount <= step_one {
            amount / DIVISOR_ONE
        } else if amount <= step_two {
            let tier_one_fee = step_one / DIVISOR_ONE;
            let tier_two_fee = (amount - step_one) / DIVISOR_TWO;
            tier_one_fee + tier_two_fee
        } else {
            let tier_one_fee = step_one / DIVISOR_ONE;
            let tier_two_fee = (step_two - step_one) / DIVISOR_TWO;
            let tier_three_fee = (amount - step_two) / DIVISOR_THREE;
            tier_one_fee + tier_two_fee + tier_three_fee
        }
    }
}

/// Health factor thresholds for UI color coding
///
/// Health factor = (collateral_value * 1000) / (total_owed * liquidation_threshold)
/// - >= HEALTHY_THRESHOLD (1.5): Safe position, displayed in green
/// - >= WARNING_THRESHOLD (1.2): At risk, displayed in amber/yellow
/// - < WARNING_THRESHOLD: Danger of liquidation, displayed in red
pub mod health {
    pub const HEALTHY_THRESHOLD: f64 = 1.5;
    pub const WARNING_THRESHOLD: f64 = 1.2;
}

/// Mainnet pool configurations
///
/// Note: Token pools (non-ERG) have empty collateral_address because
/// collateral handling differs from the ERG pool structure.
pub mod mainnet {
    use super::{PoolConfig, ProxyContracts};

    const ERG_PROXY: ProxyContracts = ProxyContracts {
        lend_address: "6ipHPSam172p6thBrRJ9AV2ZbKrv3FUQ2cJeorEK1wHL7rT9fqFLr8zHRFsL2tXidGmu6QdhyoDBLgmHy4kvGcfz1ypcbqkvMEC3ugVaHMcPCgLekExRgcbbmF3iKoLg4W81x84UaaV7zviBCUwErSSBYxyVbTdiEVkP4g2jmZvzVAcTRK16ff7xPK6XNaKf3Q4T4DE62Vx1za22bfuDtbTe7KE9DDQN17y2veMHQ38gZA1aD1cYqtDBaaV6fWydsj1aQvtDk4fESTMMjju1KRm2cENg22wDs",
        withdraw_address: "2BgqPNDeHegDQRPfME71hEmCwc8vHCrEfP5C4ybkdZnRgvQstzxRtycZvFN5VuBAipVsJ9cywKyNcGB8k8qTLURqd8Mv4kLT3mPUC6cce5HbeZfBf4wD7TKLeVaEr9oAZmhwcBEYx6L8bWfzC2rxH7szJVHSjG3WFhFJmR2CgAY9jxjVV3dpmoN8EvPvhRWmR9chPe2GitGombXyhDVnj2NhxWKWw6B6SirLSqq6D5fFsBSpQEQh3zt8zf5KKpVeHkuZnNEVQS1ZsXetR",
        borrow_address: "DoYuPfCYCWitz76iir8ihnvXgwsjVVLaTkLNkAdw4KsmVLmqHJnmxJbEjdboLoN9xa82wsZJmoboa4jB7veEgbor5C1amJiMGdkZ85bGtS4uGFNj5AWY3EP1HSpVptHjNVevjgLd1vaXfoNd56L7Ws9uUfuy7YNJbgCDidj9qSxbEcaM15N1Hb2wTsvQyqByM7cyRHvmgkaL2VY6MKo5cUbzhgJ3AdJRZUzq5Pa7wZPK4Lx2FgU1yPfVtjKdN3x5D5vTGLSgmDHMe2zQZbMWHwkznKxmJ4bio6a87MydLHjJ5XXMAEijJqdY8rdRrFwbrznu5rezkXQenDczry8Rn2VvsVmYP3FvZJQ89sybmHV9PJ6YfjHXB4Q3BkTDRLwZwfCGsbhQa1ycpaA9RBdeWjoxf5rauW7n5rGrfHt7WSzsaqCTGsXdRnkpzs33EcLf5DwBYDgKFbET8byYTyRseLQLNVPxrHmPjWsznadpqXHeScrrzb54V4pGDSVKigGxHDxrTG3pXbb9mKSwbW7xzmCUDeRtFGsfkwLVPFM295NUurf6YptKexqcESa5Rkm5KPQB689cy2vFfDEpYksUeAVGnQuWCTm",
        repay_address: "Fgu7JeiUT7BUFAkE22qXoQ1hNsmG6hNaMeRnEcmk1NedkQXTzR4xEfHgS83wkub2jrMSytnfyp5vk1MtitLL2obQH2TKQ8DZvto1B7yr3ypabKK1NsGS92svkuLa8DdKffSVCC67G5oBf7aAQqW7HA4wxsdVGmeZQpQhEDc983ojyYHHwDpLypke8a6AYAYapr7TDxoe",
        partial_repay_address: "21oVVJWKo1R7NvYL5pSTkLpKR89QHizFoXzgrA87ecoVtRBXJWhf8ZKfUrQHU4NQ97fY6W17yifLGPWCwLhQ85vmwiwGDx2njfspUDmRt1RvBRAiKHYXYbu6mBaNUEUSPzL7ZsyYNLv2aV1NfeWympbEf3jDmZUDs1Dg8A9s31LCmV2aJxJb2P5P3FS8d7sSBcQUNbtZ1Ch3aYihL6HDX1Xc6HhFFrjyzxCUQ6d2ZfecvWQCbKHQ1zasA2TuPnHchmcEgJBvP4n7xC3kby3o4xxw8bPfNaLEaqWZFbJ8DS513dsU8udpKEStXM3ze6p68ktx9PpYKZREmt4M3MFKvxVkzQKmQbuxPsKsT6D6kxbuL1yeEb",
    };

    // Token pools share common lend/withdraw/repay proxies, borrow differs per market
    const TOKEN_PROXY_BASE: ProxyContracts = ProxyContracts {
        lend_address: "KeFZdXyRkmbbDumvWSR1sijFcEbnqF7EL3YprErvspPHs3wmQ3qcD64X5JdjYARbxF4SAYivys3FgqfDgdjQNoa5ULahXEY4SAQNPPK6VLZFMGHjBmzgo7CS5MTDxcuxLvrFUZVHthy2y5DK7Bf7TqUC5TQoVFQxEgsLNpHSd5eG7s9KoGdVY2H1s4HhiEgAzUTmTqiQSaeUr4qpn8erxg8ajR74W4bVyBzovJ8oduDiHPznnrCZnZBhU3NdjLre3MDBHqEkrHnpR9hkEgACDxDLaS8cvaLXsRdejY9qaohVs",
        withdraw_address: "3rEnDaoVvvfRygpGSe76qKfAi1AHLHH8xH28rWysgHDT1TwUEcY87z8NhN7TPjshdUXCXmSxfqH3U9VTH7PoSnfVp9J9CCKKUWWtb4SQbz4SetmN7qk2JNNrMoBUfaqY2YwyVJyvjx3hszibW8wJhc8eCECpCRGUTTmrjFRvQHUYRhP47DBAVWPVgkQeMDU46rw6jYBR2aooRtyjHZKsbc8WAoNjPpdmNYjv9JtJmrMNDnsX7rWqmWWsGuv953SipQsAsAZhxezGLHGEuPKKAFruJn1Pbus2N6qT3ZkP",
        borrow_address: "", // Per-market
        repay_address: "2iHikbZ2nWVxpM6aQRXUBes3P7HZ2iZxqMMjVpsEZWSdRpnEbfPbq4ibzYkNDjyjVoh6wiusnLXApLog3dCdHuzgsys3EMcFD15zanqq1hiemHYEEVDwJMtkYyuNGseYb2ZyeogLT7ohtdfxmASYMc",
        partial_repay_address: "3X8eZkShyZ9u5h9Mid1Voz2RKgEQnWWgMtRJkqwjcWvGY5mBkwqUxQoPYzbFCSV5UhQTU3HnnwfjjUTfbaPQxUBt5yaRSzer9qHrFpYGn4M5XrwUkUuYaBd5i1LKwBd2teDz2j5iGS8myhd2MivCVzGBFcHkC4n4ry3VfxEnsqA4wtZEHecXmy9D6DPnK5yKaQkGSkUrgiFk5hL9HnW9Ae4NR29iKM4SgiSvxTYYi1oh6JUp5S5gBeR1LYNBL9VoxakABL9Pa9JHYBFzXW9YW16wWdLbh3t7a3KmtKGALgePWU6LszCPrZrXCgWZ9qMz4FB1arngGELjQ3jex",
    };

    // Per-market borrow proxies
    const SIGUSD_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZXrMBT95kNhPKRr7pyNm8CcjuyE8biuyFwKbE6f5kQT9Q2w3Fg6Jx1tk1xziDydvCPaqC19x6h5YtP2ZFYFat4bfspbsPhw52KeX8qpQbv84AJ7ZhoVvZa9aZ2w85B3tyuZNBQHipbyzFirts5UtKz7pad2savs8D9N5xrk9yoZB4vLU39Kx5fgbE48EE5au3Ze79YA6L2HiVH8GQpjyPcbE2L7kxHp88dfGg4Xarj3AqSktPb7PCdZFqa1oNLQPs8HWSdh9jpYiFtksWQvEfPkhCcedNSe7EWYB8XowyWHd9euxvXRFfeVRgSxj23hcoDGQJK9Dx58ov1gPVWTMq5T2VCA3zwMXU4emDf7rzTMPHpLud2q3a9MgFjFc5s7GhxF4Mmb2V3Wge4ZuMJN71Y4aZ1gEh6U73NMuDuUGD1eCKFHRMVZ5udXJh7SQT7m7A5dEakgYokPjLi56oNPXdSEjiqC3C85PbsJJoHyBuyCf2yrAM26wiFpRNXyJkuquW8ra66jhqdupiVYyZBzohfbyTy9ykhkiikZDJv5gUjCXCQpPkMHccNdSWBP69yioqfunS5PPdj1xfJN7xfLga8a";
    const SIGRSV_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZY4f3jsrhadauHNxMrqCBqAXeak2MVp2ig35iv7pL32WQX2K2J4xCRKa9yWDdo41x8R5gkjkPCkBpZN1eyFkqseqg1yt6mtmSxQyKt2o3ikR5VhS2HBFFy5TcRxdaj2yySWV83XCjPXvCstJLCevdoroYPbC5NbTukcgqrDuaiYaYqX6DPweNDamFJC7SUSnmWKf5ALdMUoqF5XsuewEMoTGvJvEZzS5TwXpVyxtjwoaFvPK9FJK22KAo9Ed26gUw1C727RDbQWAvirT7fPiMuoxaLSEDFv5RW3ZGUYvcPPpfJXMf7Jj7w6ZJYbpMVzi9g29deWhD5qPgf5mRR7iRWs2GUeVJEFvJAGx5ZxMp4kgGia8gFYAhEVGiAwCFPTETWokRfhrc3c8TsuV7nn42meqUkyBsDNjgXHWhjJGN7iWM4qVQEiRRKkXhaYckW4GD3PbvPZ2Au61wCL6oQNW8nBZUg2bcTZQRWaUrcKpr6n7DMHiRL9jjUGZfnDB8DhM9JviGUDp83LXM9UviremuU4aUZzX14eici4bhkyVJ7cakWY5SqxS8KfFAmpFPKtaHjFmniFvLVnTgdvCSaCfndh";
    const RSN_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZY6BCGVpPxJr9rLhVq8Gh5jMeYgR23ohyCWaDiWkHBz6kx3yGcK4tESubJyJyhgbeTq8SVQVMZWLu4yqmwLSyQJXVgVpZL1piNTazoRpyei5kyaxpwc9d7yFUDdWAySsfYYAYQoKmmkdtwUJVJks8jy5xaQZo6SxDgmKsHwkKGNkGAupntLTXDSVjb2MGJywUzSwGsWoKV67wQdNiEvjztiwuaKEqEDb8zZkFCGMaSoYcmaYEZzhuJyZartVukoUHou9SFKb11t1kuuqjcBEFMSTkmYSbncQgoizTepDUfkhQEYGtaoD6vzEFcb3c2cUvrfWp6bh6X4Zxb59iXSnEX3XzajF1HHfkEjKD5EPgawauZaZQfhQZVueHyU5UjBeYESQmxgu4FattCgEuJtBc7o4MNCG2S4f2WhDBHaR253TA8AfxLrfGmsQQXLUPVzQWNxCQ9cf2u5Lcbf7JebRxufiSrEw5zi3iEWqzNAq47wFhhTC2vihceErpWsKJvsHkbL8nQ5YSjLGi6c16nYAwWcrwuFsM34pQMtc4VN1HULce8p2N5QvX6MCc3EG1X13a3yYNyVwkXa9SSqjQsxZ7du";
    const RSADA_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZXu5gEJaEKAKHHpPB4AT5u9yrdNBfWzwNbYgz2pqJ357YGJgmpE6qn9WKGwsUFsgzF8PNqaTkiKS6qQjznqHYGW5xypwo9u9xBD9QGa9Nk8EidoggAGXp7kqL37QMy5LUVC1GJ8HzvWCkXEHokc1FiR6yJcStmsfFf7ZRCnnVtaX4fNihEhvxx5pd7D6fBpGbjPFrEC5ZcjrGpNj2NRRKgBYsTHJ2MjboLmYdXhikKLVug4VzroaT2LrCXUJDkRCNbaJYtGMFqi8BKaQQVbcG7XyzsLV1dV5rGnuAWqaxLBYaHrJLQZv2BDSRp6sxCu4N7VDfp9aRtSy3BK16UUCjnSqLYrWEkyEpzenRFbkaDUN759zNBvsxkKJ5L4Ti3GmLMMbQ1dcQCbRxU5Y3qsfnzEGJxmh69Lg3zRcgrMbeocwdYtnWTLb9a5vA26kyHnzm7tJeY7bpDrYGox8YPVk1A6deVhtGg4Uj57VNJs7byMujweg5xAurHDgpk1qwRN9tFMeuzCYXXTPDkMYpxhJYGs3vEkwjftrcyEb9uR8gVrZ89hNG9n4ujqgMoMDbAQFbZpJ3zj3h8yid87EhaqE1Y9";
    const SPF_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZXyiAE71Q1vCehs4mBzCaE8C5W5qvsQK2ddrtAaDStZGMyFc7b8W8HsBF1iubKXgXU2x9poUxoeDT6ppYVXvU2UVLbm7Gdr9ZPd8E7LHo8HWci26H1u8c6b97a5HLpNzMdE5XBjvbfpN9eYg65fraGe6AUX6cAnhTFMYjyzpPgZAR4ibuf3a3YLTD4kT7R7iKWjdXUiMtTxV6u4vNbLZNrwkQ4tx6sHocPPNuS2czHcwYPfqJTwAR77g7eprD5DL2rKteuNPAVQCenVzKQgsUH5w8a4DquWHpcpcsAg5REi4gU6bAuKuoMb9SDgyrMUmzRMoUhKWtceDDPvxaDsSdeg2VBq4EY91cVjmzFqs2TBYUmiUSq3LYotgFVKVetUPg1XaoBpqzmkDZMM19x2RMLJbSgT72cL8UP6Pj8HuH89bcAWYGgF7ZYEo5dGuB9YyDbnFgfuwTAcFCiALLWGEiC28HnCzJXiwCTjGTRnijj162BGuxEi642MChA7xQkKja88mb4ZkUhij7MbtjFDWeif4vrZPRkUG5meHFva66Er857Q9j7GEjQibmk3meVJimqg2t75zNK1Nu6MCZkUJXGA";
    const RSBTC_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZXvdSHAZZ8SUFt4qQT7BwM28gNrGfrKEMV3NQ4P3vU8xMxFJYJYm7WmMDfY8HzHxj2bCp15RY39ysCoxi86EEgBmVWd8bMG5dpYtpZXNWmRxHcnCi3bmA8w3Ame1bWmaYEiUzEiFkvLUxFWUxoj4YxkWxzz4nVkyNK29XCS5frYMJgZj51Pk6xHUELdoTX1RJCLQ8u9maWwrUtXqnW4DY3XWLZUWZiuGe1Tc5TX4Ti7a7zUF16rFBKqrmrcmxzoVXZiZf5AAFqsFftFH1PNUUh2FjjHMnjJghxJUEeLMSGFMcimeaysurDtr1aYmL1pU8Viy7qWy7WTNotexEuaKjKyFcWa3p8QQ7GHBmxcPavjszmdr9SnbRbbp3xJtcKCqQsdZvJXrHT3ZXumGQm2PLqGU7dFhQvxfYsvoDdMG2DdfEusCirzVqn2a62FtDdF1SFFoVT1JcMcP76qg2s6rXz5tjxVSE7tWPYkZwKiks73DrSEJPUUqdfPfPbEBrjckTF4Sv91Xmh2i3NbfaMKvhJAGfae9P9mEZvqv2S6RLNca8obroCZwBBRatQMuJkr924rCDBgZarSER2qP34Wr193";
    const QUACKS_BORROW: &str = "5hz4vVihRAd3zGRdVa9s4scCZwZXv4vW1MX7ubMWNf26oKbodkq1ed3kQtiJTkX1mxL1ZEqj4kaFwoQVLqWR7UgCsiUSaYkZyDt5oH3X2BfUp4tmD67eHxJVwPSPECSgrHgLVuhqQRGHgKrKuSvU5gDzEQvH2FBUd5x5X5FnRp9PgcUf2TrYRuYsvFBFezxPsKEEdMoVeFof6cjp6yyMosWqfSXEY6JCyNqJV3PFQAdv2s8oi5M4NP7Jx3j7wEj4tvdXKCU7rd7Fp7iMwwcJBh7dHuRMuNEqaFgvv5bvNdrV7T18Tc66tb6R5TNgUN6NmKb3gZQfgPxoZ1xWRQWUp2p9dHz327omEerpYch1U3LF3qemTCt8ZmFRdoUHVAsY5cK1mFeDNfkRHZqsUVAhEUduGYkrys2D33C6ZSmJCfWhnUENWd7jjNXA5fJDcwbhqmQRatgpdvbaNni7SPhU4RuLkH2k6pq7EpezMbahZwybonJkpzumbXhxgmWeLvAQVcMxNR9UWnusdJysNe1K56ZaKCE1n7spBeRV59wPamvx5csyUKNc4SheY7eg2d999gScv9RALFRHptoU71hax4dToiX5P728zLKnahpbasXpabUQt";

    pub const ERG_POOL: PoolConfig = PoolConfig {
        id: "erg",
        name: "ERG Pool",
        symbol: "ERG",
        pool_nft: "90290924d95d699f5852d54dd5c20d01a3c729b11e7ccb5444671f62bec3b4bc",
        currency_id: None,
        lend_token_id: "fc888e0eed50a4042324793a7894134d83c7aaf5c99f4bf643e7e2b4e71e0095",
        is_erg_pool: true,
        decimals: 9,
        child_nft: "13badb3b0c304a6d5859f70ac341d5b2b4ceb0be4c640ae6d1ad3a8a1edf6285",
        parent_nft: "5255c5b1da74994236ee5d737516d02839f43cc6b61b206e1dc19ed39d9c11b8",
        parameter_nft: "c75bf10aba4094f0cca74e3ff3cee0d016c2651e9554c95aabbed844f848ffe4",
        borrow_token_id: "d90d4000ed4b826856b93fc3d1e2c10ecb8a08dc0172fe72f58c43d28e681b49",
        collateral_address: "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",
        repayment_address: "YFWk9RGcvBZWrfRvAHd9uRQWrdW42fTb1TNhJDAoVCQgASqQmhdVB287nhHKGY5toAFQWUVuYGe5G7CAeqpNNAsDvEKeZLXTK6WAS7SBDp51Gt7LGubSe7KvK6X3hi6WwN6SSJ4DXpZ85gnmSPH9heL9TxW1GPjtNtijMGmeh87ozqQ3QERx1o7QAYKf3UiJWth8jbCq7qt5pkEQNZzihkTf1KFXKLFP5xbGXWBLwzJMvAF7Ubpna2oPoXN4phAyyRXQQDtN",
        proxy_contracts: ERG_PROXY,
        liquidation_threshold: 0, // ERG pool: no borrowing against it
        collateral_dex_nft: None,
    };

    pub const SIGUSD_POOL: PoolConfig = PoolConfig {
        id: "sigusd",
        name: "SigUSD Pool",
        symbol: "SigUSD",
        pool_nft: "6a5506ff2e12fe121686dfb5089b3576d0d921caba2eb68de99f7aa54c18d658",
        currency_id: Some("03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04"),
        lend_token_id: "99fd3c29dd4485bcb9cabd3574a66435a8c699bef8783ce71bc3edbb7b39e4cd",
        is_erg_pool: false,
        decimals: 2,
        child_nft: "35d6f883bc9b09cec95de38bc3b7c5d01d519b88ad3512bba6643eb5c1090780",
        parent_nft: "d45bc0077acf66b60e59d24d181e29570955c830399ede244d59d5ff726ad18e",
        parameter_nft: "6f06df3c298408925bf71a76778bc5a9c5ec551065c82ef608f531887d424d74",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf5B6ZghtHxnav9bFnQrebMKQKZQnvbkMwnTuoPp8wyvH9zoykUxkLquJkBUUsZie2Gc3Fs2rUQ2vV9ghvCfYx78bN2f2qcb9pFZoysqfuRfQs8w9rVMyDoWQ7qSWajedPzHbXpQaLTWNdJuTsuYN824KaFrrdqauhk7GQoegTmhq9tXDjTnMXXnRUxzxdcBjZfJM36XYu2kLf8ZsK3q5A7Mz9N6oa7Gg21qYpSmS4EJcagqDk8kinGmu9i6RYeXDnT6cxyd2w2eBmGy5Nd3JKzvPcy2DVRk9Th1yXhgztKu5dqkN7MW9oxA94eUgR3P2drbn4arGQiYc7",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: SIGUSD_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("9916d75132593c8b07fe18bd8d583bda1652eed7565cf41a4738ddd90fc992ec"),
    };

    pub const SIGRSV_POOL: PoolConfig = PoolConfig {
        id: "sigrsv",
        name: "SigRSV Pool",
        symbol: "SigRSV",
        pool_nft: "7551aef0a24153387c02b37cafb78aec7a852f092a23d9012bb1d06b479d42a3",
        currency_id: Some("003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0"),
        lend_token_id: "664b47cb6e1021c783be9a908e58c2daa3120a8b81f4788459e1ac3c11596902",
        is_erg_pool: false,
        decimals: 0,
        child_nft: "b533a477f97e8cb51b65e79fbb0ee7dcb232f62bb9b1f4057789d60109daab02",
        parent_nft: "3d2878c9b6ac700d3ae0dc6dc9b940acba75dc5b826aaacf0457603bf495d48f",
        parameter_nft: "a626d1d9d4eea2f809ecf8e2612af527215908421b65c2a853bca293e20d92a7",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf5BQiSNLp7zap6YGx4YVNY3wezAaUWTVkwNhK5228SgvZ3oUJ3iEQqi5D6Gn9Kc9Stfji1gkdXovyCzWyd6cn9AiVfz6mzuLU3D8qTwQp83GUJNtvpBdsizYjugY8ZWVWWNAhN4wEhgPKoo1PXi9JV4WnGu4cuMRWdpG7qzh7cHM2qdUvXZjJ7B66yirMUFZEdwHDqpWZsSTe2pxRWm7Kc71cUCo8bjG2JNuiFutBameVTTbKBUDH33XwNn1aDhzQj7gNQbVXuVvygdjGyk1PFfm3oP5W97RkFH9riGiQz9Tinxq75yogf6SY7db34xC4oqXPoZ3qCqtB",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: SIGRSV_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("1d5afc59838920bb5ef2a8f9d63825a55b1d48e269d7cecee335d637c3ff5f3f"),
    };

    pub const RSN_POOL: PoolConfig = PoolConfig {
        id: "rsn",
        name: "RSN Pool",
        symbol: "RSN",
        pool_nft: "02a406e0fc7c2bcd8438f53de3e8c4f70d272be21c5e7989615d27f8ea48bb98",
        currency_id: Some("8b08cdd5449a9592a9e79711d7d79249d7a03c535d17efaee83e216e80a44c4b"),
        lend_token_id: "acf045d8a7b468384f5b92ac174666443bfbcb80984b9c077b4ea5caca7ec2ef",
        is_erg_pool: false,
        decimals: 4,
        child_nft: "b9f70a8e4d106e7cfbab24dac530267ef242fd365c3e280ab805bdcc827bb5b6",
        parent_nft: "06e1d8fb48b9f6ad335633054601d3b9d49abae077011affe6973aa2267eab94",
        parameter_nft: "9a31cbac6a714a144a30cafb58f85a854908c62b1d923ff85fb2b2a98c4fe8ac",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf589G7zj8Yfw45X8TkWiJnUV1SHvhWaUwPpC4Um4WrF26DBNNYqkoSStYVjju3DX9u1GaaUVKnAPE6hFVxX2Xkt6bcygdPaEsAimCoYZrALq8LbrX8WXHnM5CChUvBXsjKZyPqqZThb5efbuSwgn8GeR2RkHGkT9CUtQrd6RN4Wn2buX55zNmaoUDVrdi5c8t3rJbERYy4S2pwn4i8xbAnCquNS65jk6J26EP3Lzs7yxqfiXBXJW37LDDwANwKchUJmzXfx2kkSB5XLSycYMRZZ2rSyyw4esZdUrtQZdiRBsyKP37rbmsU4Qjs6zkbRXijBbTAks2YmWA",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: RSN_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("1b694b15467c62f0cd4525e368dbdea2329c713aa200b73df4a622e950551b40"),
    };

    pub const RSADA_POOL: PoolConfig = PoolConfig {
        id: "rsada",
        name: "rsADA Pool",
        symbol: "rsADA",
        pool_nft: "84f30401051b972faac719b3589a0978fa54a7b721c7d6b40053ba4aeefad1db",
        currency_id: Some("e023c5f382b6e96fbd878f6811aac73345489032157ad5affb84aefd4956c297"),
        lend_token_id: "a8fa0b48932160bf4022dd54fe4fb3c64b233b656716c570dc048e81688e43f1",
        is_erg_pool: false,
        decimals: 6,
        child_nft: "35f826497f8eadf5b46f768485cec175c35c11360b4821ea92bbbe855777b55c",
        parent_nft: "5ef4bc9e075f8113416ff544efeca7d397bc03e142a7c3816fc00bc494603b85",
        parameter_nft: "eac027ed8167d565eb4b62f6b8de18ec727d9d7ccfc246f95c59ea72b9975a30",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf5BrY6tCEatJcxe4n1qhprKB1ddNoqzmCDSs7V9eRUWPFm6v71PjkWuVKLHZ92sr6rcFazpF6dNVgPWSvciygUhkcWndC2couXUdjggTH2msyogK2fCjMJSWFDeusyRgjGTkHtwiy6EzxS567ULxKMJHaNSTiAJZUJfoafyforFVHnmdgkziTTtU2GN1ADCeEuX5ivRnrzr7fiedptUW5WHggMWHAvC7qDpMkPzed7cwBUR72Jvtkdp9Ag9PyioDWho91Qt4rNiAbWANRo4ByWD2A6hfY87s9SEK74nuJckF5wnVTv3qzq7tEhHqrF5JTRDnzkCiHmTbN",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: RSADA_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("ae97c5eccd59a065cd973a8d6afb8bb79f9cc70368a7dcdf73aaeab1cedf6f6b"),
    };

    pub const SPF_POOL: PoolConfig = PoolConfig {
        id: "spf",
        name: "SPF Pool",
        symbol: "SPF",
        pool_nft: "0bd05d8f87dfd1e834f01e11ef2c632a62dc525ac5c9d6edce988e0f4709bde7",
        currency_id: Some("9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922269913e0de369d"),
        lend_token_id: "fb4bc066802562b66b170ecf9332e5b258897b49114b5436ee58f188bbdc1af0",
        is_erg_pool: false,
        decimals: 6,
        child_nft: "d34691ed46424f9b2f6911da7467ba9c0b14e2fafea161b6df30eb9ea12d5096",
        parent_nft: "5297efeab26c230b81f22fcd99836550d5bbbc740453cde190a0b0ba9467b1a2",
        parameter_nft: "4a6b0475bc6244ce18c5e1c99134252490e9d644a6b2034223ac3445aa85795b",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf58QR5qeALLvHMbvcCNKVcaj3J4Yr3vwom88QMVEqFHorLKWynjBJadudNgBDU38zaaSkRJ5AC9ewYePHVNFaTHu2fhvWMMV4r34CG3DAjzaYcrFDqq8ozxLXmcB2RHhevknMJLt93SN3BW3sGe4wLeMKfc6rq7ifjbKhEnBo9qojr584LiK9A3hWnfPDY3BpdkMStaMh9nmWG5VoZEDWUth8wgesZfjCsm44XmKPJUguuprh6e9nVBnw8EuEirCSNgiuWizJX7kxGXMm5p9fcU1MMybmfGFFwUmbKBDrXCSD3x6k5NGVWKhccVyWmhK3GcLBJv5YAm38",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: SPF_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("f40afb6f877c40a30c8637dd5362227285738174151ce66d6684bc1b727ab6cf"),
    };

    pub const RSBTC_POOL: PoolConfig = PoolConfig {
        id: "rsbtc",
        name: "rsBTC Pool",
        symbol: "rsBTC",
        pool_nft: "585ab31dc331eac20a8ef2a201109b019b0758568fa0f455ddcf42d930fccc7a",
        currency_id: Some("7a51950e5f548549ec1aa63ffdc38279505b11e7e803d01bcf8347e0123c88b0"),
        lend_token_id: "9252c6339313da4fd95e30b9bf6726e9bf008dd1a5d33577351cd6ee2a2d74db",
        is_erg_pool: false,
        decimals: 8,
        child_nft: "4ae605efc36423c972ed2615ba04a7e35851551f26a71b9dd470aa92242bbd62",
        parent_nft: "4fd84dd90aa0c55d49cb0c1f784b8fb6eaa97dad2a97e130829b963ffdbc0162",
        parameter_nft: "fcfe22b8d4655b88a8a39582f81fe2eb2c05f2e4f3eb8b627d9217af94731e22",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf5Aas5F5rVyjZNxgPJhVCcZXeZH8tD7Nr3ninpnqxAbuYjQDRzVb9LztDxwyNYaPqjodNe1MEkiUynpo7CqQATcZX4hv3j6bF9us9NHsooko7XuzybtZhHFh8XVa2TmaafYyWgchLMPLEb88dyTnDR1g6TWAdzSt9n1uti9PUJVjV8nRhacKoQ81CPV8jADoF9sMAAC48e1cnnZEWNrXRXjbKb25hf8wvEkiEo695GzNSeoU7zzzBRqL3WTpY3TGi2jrgi14GewDzDLRQ9fzSZ85v99e4GzfT7gmyRRcCm4nC5XqGcUnysGeAxEvq1aUCNWJo5NyjnMQt",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: RSBTC_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1250,
        collateral_dex_nft: Some("47a811c68e49f6bfa6629602037ee65f8d175ddbc7b64bdb65ad40599b812fd0"),
    };

    pub const QUACKS_POOL: PoolConfig = PoolConfig {
        id: "quacks",
        name: "QUACKS Pool",
        symbol: "QUACKS",
        pool_nft: "a694087fd21779c0e4319555d3dcdf68c2110b7588f5f17e4688aaf57285e1b1",
        currency_id: Some("089990451bb430f05a85f4ef3bcb6ebf852b3d6ee68d86d78658b9ccef20074f"),
        lend_token_id: "acee078a7a8c31cbd5718f0230984b1dcd32a40e48a148d7fbc070f478a4073a",
        is_erg_pool: false,
        decimals: 0,
        child_nft: "d0e054cd5d37edf924b264a8286af72b05a48669bd04acadaaaffb2c7fdb8272",
        parent_nft: "c59315df37501762ec2878f0ada22e10cfab36bc82269e02da1d4c02f5eaed4f",
        parameter_nft: "dd9e9a988754437420da3078cedfdc21c37a4b4fde76b48a6e42cb7852e118e9",
        borrow_token_id: "",
        collateral_address: "",
        repayment_address: "r5zW3yf5Cp6PAiK1cgDEj9k4tExPLMR9jAFGR1QXD3NtVvAhRDLgyGCGn82JJwzgV7v41zaN3NmoWDrk2nH5SiQGE4pafSc9aWBWDn256v2iZWrU6rPGbxbs3arTMHL4rxq3DW5qQ31TUU9MGcRaq2dt2nUdWLoQNp1LLDrtsNM57ToLd3VV3WnBEPzHjq64xvb1MoyME2QaCNGUH6vsQw2a51YZAUajC2T7M38RQ3mNsD9nfp7WgikTuJrK9WRqRNXBXPy8mrSWGSthCo5cyQKjs9NVtBcaFMsuiCTqYGJRT1SySxcKF1fPAYjoCf7uTHE5WKkTZkz39jPmZK6",
        proxy_contracts: ProxyContracts {
            lend_address: TOKEN_PROXY_BASE.lend_address,
            withdraw_address: TOKEN_PROXY_BASE.withdraw_address,
            borrow_address: QUACKS_BORROW,
            repay_address: TOKEN_PROXY_BASE.repay_address,
            partial_repay_address: TOKEN_PROXY_BASE.partial_repay_address,
        },
        liquidation_threshold: 1400,
        collateral_dex_nft: Some("46463b61bae37a3f2f0963798d57279167d82e17f78ccd0ccedec7e49cbdbbd1"),
    };

    pub const ALL_POOLS: &[PoolConfig] = &[
        ERG_POOL,
        SIGUSD_POOL,
        SIGRSV_POOL,
        RSN_POOL,
        RSADA_POOL,
        SPF_POOL,
        RSBTC_POOL,
        QUACKS_POOL,
    ];
}

/// Get all pool configs (mainnet only for MVP)
pub fn get_pools() -> &'static [PoolConfig] {
    mainnet::ALL_POOLS
}

/// Get a specific pool by ID
pub fn get_pool(pool_id: &str) -> Option<&'static PoolConfig> {
    get_pools().iter().find(|p| p.id == pool_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_pools_exist() {
        assert_eq!(get_pools().len(), 8);
    }

    #[test]
    fn test_get_pool_by_id() {
        // Verify all 8 pools can be retrieved
        let pool_ids = [
            "erg", "sigusd", "sigrsv", "rsn", "rsada", "spf", "rsbtc", "quacks",
        ];
        for id in pool_ids {
            assert!(get_pool(id).is_some(), "Pool '{}' should exist", id);
        }
        // Verify invalid pool returns None
        assert!(get_pool("invalid").is_none());
    }

    #[test]
    fn test_erg_pool_is_erg() {
        let pool = get_pool("erg").unwrap();
        assert!(pool.is_erg_pool);
        assert!(pool.currency_id.is_none());
    }

    #[test]
    fn test_service_fee_calculation() {
        // Small amount: tier 1 only
        assert_eq!(fees::calculate_service_fee(1000, fees::TOKEN_THRESHOLDS), 6);
        // Large amount: all tiers
        assert_eq!(
            fees::calculate_service_fee(300000, fees::TOKEN_THRESHOLDS),
            1402
        );
    }

    #[test]
    fn test_service_fee_tier_boundaries() {
        let (t1, t2) = fees::TOKEN_THRESHOLDS;

        // At tier 1 boundary - fee switches to tier 2 calculation
        // Need to go past by DIVISOR_TWO for integer division to show increase
        let fee_at_t1 = fees::calculate_service_fee(t1, fees::TOKEN_THRESHOLDS);
        let fee_past_t1 =
            fees::calculate_service_fee(t1 + fees::DIVISOR_TWO, fees::TOKEN_THRESHOLDS);
        assert!(fee_past_t1 > fee_at_t1, "Fee should increase past tier 1");

        // At tier 2 boundary - fee switches to tier 3 calculation
        // Need to go past by DIVISOR_THREE for integer division to show increase
        let fee_at_t2 = fees::calculate_service_fee(t2, fees::TOKEN_THRESHOLDS);
        let fee_past_t2 =
            fees::calculate_service_fee(t2 + fees::DIVISOR_THREE, fees::TOKEN_THRESHOLDS);
        assert!(fee_past_t2 > fee_at_t2, "Fee should increase past tier 2");
    }
}
