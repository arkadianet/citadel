/** Standard miner fee (0.0011 ERG) */
export const TX_FEE_NANO = 1_100_000

/** Citadel app developer fee (0.011 ERG) — separate from miner fee */
export const DEV_FEE_NANO = 11_000_000

/**
 * Default Citadel fee / tip recipient (mainnet P2PK).
 * Keep in sync with `ergo_tx::DEFAULT_DEV_FEE_ADDRESS`.
 */
export const DEFAULT_DEV_FEE_ADDRESS =
  '9eoLQ6FFKJPqZXeBFvd3CKu7DRfXavKo7n9PFkVypSmXgD6ActU'

/** Minimum box value (0.001 ERG) */
export const MIN_BOX_VALUE_NANO = 1_000_000

/** Duckpools proxy execution fee — higher than standard to cover bot costs */
export const LENDING_PROXY_FEE_NANO = 2_000_000

/** Combined fees reserved on wallet/UTXO txs (miner + Citadel) */
export const WALLET_TX_FEES_NANO = TX_FEE_NANO + DEV_FEE_NANO

/**
 * Max user-defined output slots for UTXO Restructure.
 * App cap (keep in sync with `ergo_tx::utxo_management::MAX_RESTRUCTURE_OUTPUTS`).
 * Protocol allows up to 32_767 outputs; block cost may bind earlier.
 */
export const MAX_RESTRUCTURE_OUTPUTS = 120
