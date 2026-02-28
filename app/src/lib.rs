//! Citadel Tauri application library

pub mod commands;
pub mod tx_watcher;

use citadel_api::AppState;

use commands::{RosenConfigState, RosenTokenMapState};
use tx_watcher::TxWatcherState;

/// Run the Tauri application
pub fn run() {
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("citadel=debug".parse().unwrap())
                .add_directive("info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting Citadel application");

    let state = AppState::new();

    let rosen_config_state = RosenConfigState(tokio::sync::Mutex::new(None));
    let rosen_token_map_state = RosenTokenMapState(tokio::sync::Mutex::new(None));
    let tx_watcher_state = TxWatcherState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .manage(rosen_config_state)
        .manage(rosen_token_map_state)
        .manage(tx_watcher_state)
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::get_node_status,
            commands::configure_node,
            commands::get_sigmausd_state,
            commands::get_oracle_price,
            commands::start_wallet_connect,
            commands::get_wallet_status,
            commands::get_connection_status,
            commands::disconnect_wallet,
            commands::get_wallet_balance,
            commands::get_recent_transactions,
            commands::preview_mint_sigusd,
            commands::build_mint_sigusd,
            commands::start_mint_sign,
            commands::get_mint_tx_status,
            commands::get_user_utxos,
            commands::preview_sigmausd_tx,
            commands::build_sigmausd_tx,
            commands::open_nautilus,
            // Dexy Protocol
            commands::get_dexy_state,
            commands::get_dexy_rates,
            commands::preview_mint_dexy,
            commands::build_mint_dexy,
            commands::preview_dexy_swap,
            commands::build_dexy_swap_tx,
            commands::preview_lp_deposit,
            commands::build_lp_deposit_tx,
            commands::preview_lp_redeem,
            commands::build_lp_redeem_tx,
            // Duckpools Lending Protocol
            commands::get_lending_markets,
            commands::get_lending_positions,
            commands::build_lend_tx,
            commands::build_withdraw_tx,
            commands::build_borrow_tx,
            commands::build_repay_tx,
            commands::build_refund_tx,
            commands::check_proxy_box,
            commands::discover_stuck_proxies,
            commands::get_dex_price,
            // AMM Protocol
            commands::get_amm_pools,
            commands::get_amm_quote,
            commands::preview_swap,
            commands::build_swap_tx,
            commands::start_swap_sign,
            commands::get_swap_tx_status,
            commands::get_box_by_id,
            // AMM Direct Swap
            commands::preview_direct_swap,
            commands::build_direct_swap_tx,
            // AMM Order Discovery & Refund
            commands::get_pending_orders,
            commands::get_mempool_swaps,
            commands::build_swap_refund_tx,
            commands::start_refund_sign,
            commands::get_refund_tx_status,
            // AMM LP Operations
            commands::preview_amm_lp_deposit,
            commands::build_amm_lp_deposit_tx,
            commands::build_amm_lp_deposit_order,
            commands::preview_amm_lp_redeem,
            commands::build_amm_lp_redeem_tx,
            commands::build_amm_lp_redeem_order,
            // AMM Pool Creation
            commands::preview_pool_create,
            commands::build_pool_bootstrap_tx,
            commands::build_pool_create_tx,
            // AMM Smart Router
            commands::find_swap_routes,
            commands::find_swap_routes_by_output,
            commands::find_split_route,
            commands::compare_sigusd_options,
            commands::get_liquidity_depth,
            commands::get_sigusd_arb_snapshot,
            // Explorer
            commands::explorer_node_info,
            commands::explorer_get_transaction,
            commands::explorer_get_block,
            commands::explorer_get_block_headers,
            commands::explorer_get_mempool,
            commands::explorer_get_box,
            commands::explorer_get_token,
            commands::explorer_get_address,
            commands::explorer_search,
            // Token Burn
            commands::build_burn_tx,
            commands::build_multi_burn_tx,
            commands::start_burn_sign,
            commands::get_burn_tx_status,
            // Address Validation
            commands::validate_ergo_address,
            // UTXO Management
            commands::build_consolidate_tx,
            commands::build_split_tx,
            commands::start_utxo_mgmt_sign,
            commands::get_utxo_mgmt_tx_status,
            // Protocol Activity
            commands::get_protocol_activity,
            commands::get_dexy_activity,
            commands::get_sigmausd_activity,
            // HodlCoin Protocol
            commands::get_hodlcoin_banks,
            commands::preview_hodlcoin_mint,
            commands::preview_hodlcoin_burn,
            commands::build_hodlcoin_mint_tx,
            commands::build_hodlcoin_burn_tx,
            commands::start_hodlcoin_sign,
            commands::get_hodlcoin_tx_status,
            // Rosen Bridge
            commands::init_bridge_config,
            commands::get_bridge_state,
            commands::get_bridge_tokens,
            commands::get_bridge_fees,
            commands::build_bridge_lock_tx,
            commands::start_bridge_sign,
            commands::get_bridge_tx_status,
            // SigmaFi Bonds
            commands::sigmafi_fetch_market,
            commands::sigmafi_get_tokens,
            commands::sigmafi_build_open_order,
            commands::sigmafi_build_cancel_order,
            commands::sigmafi_build_close_order,
            commands::sigmafi_build_repay,
            commands::sigmafi_build_liquidate,
            commands::start_sigmafi_sign,
            commands::get_sigmafi_tx_status,
            // MewLock Timelocks
            commands::mewlock_fetch_state,
            commands::mewlock_get_durations,
            commands::mewlock_build_lock,
            commands::mewlock_build_unlock,
            commands::start_mewlock_sign,
            commands::get_mewlock_tx_status,
            // Node Discovery
            commands::discover_nodes,
            commands::probe_single_node,
            // Transaction Watcher
            tx_watcher::watch_tx,
            tx_watcher::watch_order,
            tx_watcher::get_watched_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
