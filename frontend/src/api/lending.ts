/**
 * Lending Protocol API
 *
 * TypeScript types and invoke wrappers for Duckpools lending protocol Tauri commands.
 *
 * Commands:
 * - get_lending_markets: Fetch all lending pools with metrics
 * - get_lending_positions: Fetch user positions for an address
 * - build_lend_tx: Build lend (deposit) proxy transaction
 * - build_withdraw_tx: Build withdraw (redeem LP tokens) proxy transaction
 * - build_borrow_tx: Build borrow proxy transaction (returns error - not implemented)
 * - build_repay_tx: Build repay proxy transaction
 * - build_refund_tx: Build refund transaction for stuck proxy box
 */

import { invoke } from '@tauri-apps/api/core'

// =============================================================================
// Type Definitions
// =============================================================================

/**
 * Collateral option fetched from on-chain parameter box
 */
export interface CollateralOption {
  token_id: string
  token_name: string
  liquidation_threshold: number
  liquidation_penalty: number
  dex_nft: string | null
}

/**
 * Pool information with metrics
 */
export interface PoolInfo {
  pool_id: string
  name: string
  symbol: string
  decimals: number
  is_erg_pool: boolean
  total_supplied: string
  total_borrowed: string
  available_liquidity: string
  utilization_pct: number
  supply_apy: number
  borrow_apy: number
  pool_box_id: string
  collateral_options: CollateralOption[]
}

/**
 * Response from get_lending_markets
 */
export interface MarketsResponse {
  pools: PoolInfo[]
  block_height: number
}

/**
 * User's lending position in a pool
 */
export interface LendPositionInfo {
  pool_id: string
  pool_name: string
  lp_tokens: string
  underlying_value: string
  unrealized_profit: string
}

/**
 * User's borrow position in a pool
 */
export interface BorrowPositionInfo {
  pool_id: string
  pool_name: string
  collateral_box_id: string
  collateral_token: string
  collateral_name: string
  collateral_amount: string
  borrowed_amount: string
  total_owed: string
  health_factor: number
  /** 'green' | 'amber' | 'red' based on health_factor thresholds */
  health_status: string
}

/**
 * Response from get_lending_positions
 */
export interface PositionsResponse {
  address: string
  lend_positions: LendPositionInfo[]
  borrow_positions: BorrowPositionInfo[]
  block_height: number
}

/**
 * Request to build a lend (deposit) transaction
 */
export interface LendBuildRequest {
  pool_id: string
  /** Amount in base units (nanoERG for ERG pool, token units for token pools) */
  amount: number
  user_address: string
  /** User's UTXOs in EIP-12 JSON format */
  user_utxos: unknown[]
  current_height: number
  /** Slippage tolerance in basis points (0-200 for 0%-2%), defaults to 0 */
  slippage_bps: number
}

/**
 * Request to build a withdraw (redeem LP) transaction
 */
export interface WithdrawBuildRequest {
  pool_id: string
  /** Amount of LP tokens to redeem */
  lp_amount: number
  user_address: string
  /** User's UTXOs in EIP-12 JSON format */
  user_utxos: unknown[]
  current_height: number
}

/**
 * Request to build a borrow transaction
 */
export interface BorrowBuildRequest {
  pool_id: string
  collateral_token: string
  collateral_amount: number
  borrow_amount: number
  user_address: string
  /** User's UTXOs in EIP-12 JSON format */
  user_utxos: unknown[]
  current_height: number
}

/**
 * Request to build a repay transaction
 */
export interface RepayBuildRequest {
  pool_id: string
  /** Box ID of the collateral being repaid */
  collateral_box_id: string
  /** Amount to repay in base units */
  repay_amount: number
  user_address: string
  /** User's UTXOs in EIP-12 JSON format */
  user_utxos: unknown[]
  current_height: number
}

/**
 * Request to build a refund transaction for stuck proxy box
 */
export interface RefundBuildRequest {
  /** Box ID of the proxy box to refund */
  proxy_box_id: string
  user_address: string
  /**
   * User's UTXOs in EIP-12 JSON format.
   * First element must be the proxy box data with full register information.
   */
  user_utxos: unknown[]
  current_height: number
}

/**
 * Transaction summary returned with build responses
 */
export interface LendingTxSummary {
  action: string
  pool_id: string
  pool_name: string
  amount_in: string
  amount_out_estimate: string | null
  tx_fee_nano: string
  refund_height: number
  /** Service fee formatted for display (e.g. "0.006250 SigUSD") */
  service_fee: string
  /** Service fee in base units as string */
  service_fee_nano: string
  /** Total tokens/ERG user sends to proxy (amount + fee + slippage) */
  total_to_send: string
}

/**
 * Response from build_lend_tx, build_withdraw_tx, build_repay_tx, build_refund_tx
 */
export interface LendingBuildResponse {
  /** Unsigned transaction in EIP-12 JSON format */
  unsigned_tx: unknown
  summary: LendingTxSummary
}

// =============================================================================
// API Functions
// =============================================================================

/**
 * Fetch all lending markets with pool metrics.
 *
 * Returns pools with APY, utilization, TVL, and other metrics.
 * Does not include user-specific position data.
 *
 * @returns Promise<MarketsResponse> Array of pools with metrics
 * @throws Error if node is not connected or capabilities unavailable
 */
export async function getLendingMarkets(): Promise<MarketsResponse> {
  return invoke<MarketsResponse>('get_lending_markets')
}

/**
 * Fetch user's lending and borrow positions.
 *
 * Returns all lend positions (LP tokens held) and borrow positions
 * (outstanding loans with collateral) for the given address.
 *
 * @param address - Ergo address to fetch positions for
 * @returns Promise<PositionsResponse> User's positions across all pools
 * @throws Error if node is not connected or capabilities unavailable
 */
export async function getLendingPositions(address: string): Promise<PositionsResponse> {
  return invoke<PositionsResponse>('get_lending_positions', { address })
}

/**
 * Build a lend (deposit) transaction.
 *
 * Creates a proxy transaction that deposits funds into a lending pool.
 * The Duckpools bot will process this and send LP tokens to the user.
 *
 * @param request - Lend build request with pool, amount, and UTXOs
 * @returns Promise<LendingBuildResponse> Unsigned transaction and summary
 * @throws Error if pool not found, amount is zero, or insufficient funds
 */
export async function buildLendTx(request: LendBuildRequest): Promise<LendingBuildResponse> {
  return invoke<LendingBuildResponse>('build_lend_tx', { request })
}

/**
 * Build a withdraw (redeem LP tokens) transaction.
 *
 * Creates a proxy transaction that redeems LP tokens for underlying assets.
 * The Duckpools bot will process this and return funds to the user.
 *
 * @param request - Withdraw build request with pool, LP amount, and UTXOs
 * @returns Promise<LendingBuildResponse> Unsigned transaction and summary
 * @throws Error if pool not found, LP amount is zero, or insufficient LP tokens
 */
export async function buildWithdrawTx(request: WithdrawBuildRequest): Promise<LendingBuildResponse> {
  return invoke<LendingBuildResponse>('build_withdraw_tx', { request })
}

/**
 * Build a borrow transaction.
 *
 * Creates a proxy transaction that posts collateral and requests a borrow.
 * The Duckpools bot will process this and send borrowed tokens to the user.
 *
 * @param request - Borrow build request with collateral and borrow amounts
 * @returns Promise<LendingBuildResponse> Unsigned transaction and summary
 * @throws Error if pool not found, amounts are zero, or insufficient funds
 */
export async function buildBorrowTx(request: BorrowBuildRequest): Promise<LendingBuildResponse> {
  return invoke<LendingBuildResponse>('build_borrow_tx', { request })
}

/**
 * Build a repay transaction.
 *
 * Creates a transaction to repay borrowed funds and potentially reclaim collateral.
 * Can partially repay or fully repay a loan.
 *
 * @param request - Repay build request with pool, collateral box, amount, and UTXOs
 * @returns Promise<LendingBuildResponse> Unsigned transaction and summary
 * @throws Error if pool not found, collateral box not found, or repay amount is zero
 */
export async function buildRepayTx(request: RepayBuildRequest): Promise<LendingBuildResponse> {
  return invoke<LendingBuildResponse>('build_repay_tx', { request })
}

/**
 * Build a refund transaction for a stuck proxy box.
 *
 * If a lend/withdraw/borrow/repay proxy transaction was not processed by
 * the Duckpools bots (e.g., insufficient liquidity), users can reclaim
 * their funds after the refund height stored in the proxy box.
 *
 * @param request - Refund build request with proxy box ID and UTXOs
 * @returns Promise<LendingBuildResponse> Unsigned transaction and summary
 * @throws Error if proxy box not found, not yet refundable, or invalid format
 */
export async function buildRefundTx(request: RefundBuildRequest): Promise<LendingBuildResponse> {
  return invoke<LendingBuildResponse>('build_refund_tx', { request })
}

// =============================================================================
// Helper Types (for component use)
// =============================================================================

/**
 * Health status derived from health_factor.
 * Used for UI color coding of borrow positions.
 */
export type HealthStatus = 'green' | 'amber' | 'red'

/**
 * Check if a health status string is valid.
 */
export function isHealthStatus(status: string): status is HealthStatus {
  return status === 'green' || status === 'amber' || status === 'red'
}

/**
 * Parse health_factor to determine risk level.
 *
 * @param healthFactor - The health factor value (1.0 = liquidation threshold)
 * @returns HealthStatus for UI display
 */
export function getHealthStatus(healthFactor: number): HealthStatus {
  if (healthFactor >= 1.5) return 'green'
  if (healthFactor >= 1.2) return 'amber'
  return 'red'
}

/**
 * Format a large number string for display.
 *
 * @param value - String representation of a number (may be very large)
 * @param decimals - Number of decimal places for the token
 * @returns Formatted string with appropriate precision
 */
export function formatAmount(value: string, decimals: number): string {
  const num = BigInt(value)
  const divisor = BigInt(10 ** decimals)
  const whole = num / divisor
  const fraction = num % divisor

  if (decimals === 0) {
    return whole.toLocaleString()
  }

  const fractionStr = fraction.toString().padStart(decimals, '0')
  // Trim trailing zeros but keep at least 2 decimal places
  const trimmed = fractionStr.replace(/0+$/, '').padEnd(2, '0')
  const displayDecimals = Math.min(trimmed.length, 4)

  return `${whole.toLocaleString()}.${trimmed.slice(0, displayDecimals)}`
}

/**
 * Format APY percentage for display.
 *
 * @param apy - APY as a decimal (e.g., 0.05 for 5%)
 * @returns Formatted percentage string (e.g., "5.00%")
 */
export function formatApy(apy: number): string {
  return `${(apy * 100).toFixed(2)}%`
}

/**
 * Format utilization percentage for display.
 *
 * @param utilization - Utilization as percentage (e.g., 75.5 for 75.5%)
 * @returns Formatted percentage string (e.g., "75.5%")
 */
export function formatUtilization(utilization: number): string {
  return `${utilization.toFixed(2)}%`
}
