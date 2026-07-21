//! Live probe against local nodes — run with:
//! `cargo test -p ergo-node-client --test live_probe -- --ignored --nocapture`
use citadel_core::NodeConfig;
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_node_client::{address_to_ergo_tree, NodeClient};

#[tokio::test]
#[ignore]
async fn probe_rust_node_9063_full() {
    let cfg = NodeConfig {
        url: "http://127.0.0.1:9063".into(),
        api_key: String::new(),
    };
    let client = NodeClient::new(cfg).await.expect("connect 9063");
    let caps = client.capabilities().await.expect("caps");
    println!("9063 caps: {:?}", caps);
    assert!(caps.is_online);
    assert_eq!(caps.capability_tier.as_str(), "Full");

    let tree_hex = "0008cd0301bf02503713677872af1e5b8a2358b2ddc9b48a3f84b318348099252bb2e34c";
    let tree_bytes = hex::decode(tree_hex).unwrap();
    let tree = ergo_lib::ergotree_ir::ergo_tree::ErgoTree::sigma_parse_bytes(&tree_bytes).unwrap();
    let addr = AddressEncoder::new(NetworkPrefix::Mainnet).address_to_str(
        &Address::recreate_from_ergo_tree(&tree).unwrap(),
    );
    println!("derived address: {}", addr);
    assert_eq!(address_to_ergo_tree(&addr).as_deref(), Some(tree_hex));

    let (erg, tokens) = client
        .get_address_balances(&addr)
        .await
        .expect("balances must work with JSON-quoted address body");
    println!("balances erg={} tokens={}", erg, tokens.len());

    let utxos = client.get_address_utxos(&addr).await.expect("utxos");
    println!("utxos count={}", utxos.len());
    assert!(!utxos.is_empty());

    let txs = client
        .get_recent_transactions(&addr, 5)
        .await
        .expect("recent txs must work with JSON-quoted address body");
    println!("recent txs count={}", txs.len());
}

#[tokio::test]
#[ignore]
async fn probe_scala_vs_rust_by_token() {
    let body: serde_json::Value =
        reqwest::get("http://127.0.0.1:9063/transactions/unconfirmed?limit=50")
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    let mut token_id = None;
    if let Some(arr) = body.as_array() {
        'outer: for tx in arr {
            if let Some(outs) = tx["outputs"].as_array() {
                for o in outs {
                    if let Some(assets) = o["assets"].as_array() {
                        if let Some(a) = assets.first() {
                            if let Some(tid) = a["tokenId"].as_str() {
                                token_id = Some(tid.to_string());
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
    }
    let Some(tid) = token_id else {
        println!("no token in mempool; skip");
        return;
    };
    println!("probing token {}", tid);

    for port in [9063u16, 9053] {
        let cfg = NodeConfig {
            url: format!("http://127.0.0.1:{}", port),
            api_key: String::new(),
        };
        let client = NodeClient::new(cfg).await.expect("connect");
        let caps = client.capabilities().await.unwrap();
        let token = citadel_core::TokenId::new(&tid);
        match client.get_boxes_by_token_id(&caps, &token, 3).await {
            Ok(b) => println!("{} byTokenId ok count={}", port, b.len()),
            Err(e) => println!("{} byTokenId ERR: {}", port, e),
        }
        match client.get_token_info(&tid).await {
            Ok(i) => println!("{} tokenInfo name={:?} decimals={:?}", port, i.name, i.decimals),
            Err(e) => println!("{} tokenInfo ERR: {}", port, e),
        }
    }
}
