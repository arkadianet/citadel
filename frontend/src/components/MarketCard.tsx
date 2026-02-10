/**
 * MarketCard Component
 *
 * Displays a lending pool card with:
 * - Pool info: name, symbol, APY rates, utilization, available liquidity
 * - User's lending position if they have one (LP tokens, underlying value, profit)
 * - User's borrow positions if they have any (with health factor color coding)
 * - Action buttons: Lend, Withdraw, Borrow, Repay
 */

import {
  formatAmount,
  formatApy,
  formatUtilization,
  type PoolInfo,
  type LendPositionInfo,
  type BorrowPositionInfo,
} from '../api/lending'

/**
 * Wallet balance information passed from parent component
 */
export interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  sigusd_amount: number
  sigusd_formatted: string
  sigrsv_amount: number
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

export interface MarketCardProps {
  /** Pool information with metrics */
  pool: PoolInfo
  /** Display mode: supply shows lend/withdraw, borrow shows borrow/repay */
  mode: 'supply' | 'borrow'
  /** User's lending position in this pool (if any) */
  lendPosition?: LendPositionInfo
  /** User's borrow position in this pool (if any) */
  borrowPosition?: BorrowPositionInfo
  /** Connected wallet address (null if not connected) */
  walletAddress: string | null
  /** Wallet balance information for display */
  walletBalance: WalletBalance | null
  /** Callback when user clicks Lend button */
  onLend: () => void
  /** Callback when user clicks Withdraw button */
  onWithdraw: () => void
  /** Callback when user clicks Borrow button */
  onBorrow: () => void
  /** Callback when user clicks Repay button with the borrow position */
  onRepay: (borrowPosition: BorrowPositionInfo) => void
}

/** Map pool ID to icon path */
const POOL_ICONS: Record<string, string> = {
  erg: '/icons/ergo.svg',
  sigusd: '/icons/sigmausd.svg',
  sigrsv: '/icons/sigrsv.svg',
  rsn: '/icons/rosen.svg',
  rsada: '/icons/rsada.svg',
  spf: '/icons/spf.svg',
  rsbtc: '/icons/rsbtc.svg',
  quacks: '/icons/quacks.svg',
}

/** Map collateral token name to icon path */
const COLLATERAL_ICONS: Record<string, string> = {
  ERG: '/icons/ergo.svg',
  SigUSD: '/icons/sigmausd.svg',
  SigRSV: '/icons/sigrsv.svg',
  RSN: '/icons/rosen.svg',
  rsADA: '/icons/rsada.svg',
  SPF: '/icons/spf.svg',
  rsBTC: '/icons/rsbtc.svg',
  QUACKS: '/icons/quacks.svg',
}

/**
 * Get user's available balance for a pool's asset
 */
function getAvailableBalance(
  pool: PoolInfo,
  walletBalance: WalletBalance | null
): string | null {
  if (!walletBalance) return null

  if (pool.is_erg_pool) {
    return walletBalance.erg_formatted
  }

  // Find the token in wallet balance
  const token = walletBalance.tokens.find((t) => {
    // Match by token name (symbol) since we may not have token_id in pool
    return (
      t.name?.toLowerCase() === pool.symbol.toLowerCase() ||
      t.name?.toLowerCase() === pool.name.toLowerCase()
    )
  })

  if (token) {
    // Format the token amount
    const divisor = Math.pow(10, token.decimals)
    return (token.amount / divisor).toFixed(token.decimals > 2 ? 4 : 2)
  }

  return null
}

/**
 * MarketCard - Individual lending pool display component.
 *
 * Shows pool metrics, user positions, and action buttons for interacting
 * with the Duckpools lending protocol.
 */
export function MarketCard({
  pool,
  mode,
  lendPosition,
  borrowPosition,
  walletAddress,
  walletBalance,
  onLend,
  onWithdraw,
  onBorrow,
  onRepay,
}: MarketCardProps) {
  const hasLendPosition = lendPosition && BigInt(lendPosition.lp_tokens) > 0n
  const hasBorrowPosition =
    borrowPosition && BigInt(borrowPosition.borrowed_amount) > 0n

  // Get available balance for display
  const availableBalance = getAvailableBalance(pool, walletBalance)

  return (
    <div className={`market-card ${pool.is_erg_pool ? 'erg' : 'token'}`}>
      {/* Header */}
      <div className="market-card-header">
        <div className="market-header-content">
          <div className="market-header-left">
            <div className="market-icon">
              {POOL_ICONS[pool.pool_id] ? (
                <img src={POOL_ICONS[pool.pool_id]} alt={pool.symbol} />
              ) : (
                pool.symbol.charAt(0)
              )}
            </div>
            <div className="market-info">
              <h3>{pool.name}</h3>
              <p>{pool.symbol}</p>
            </div>
          </div>
          <span className="market-ticker">{pool.symbol}</span>
        </div>
      </div>

      {/* Body */}
      <div className="market-card-body">
        {/* Stats Grid */}
        <div className="market-stats">
          {mode === 'supply' ? (
            <div className="market-stat">
              <div className="stat-header">
                <span className="stat-label">Supply APY</span>
              </div>
              <span className="stat-value positive">
                {formatApy(pool.supply_apy)}
              </span>
            </div>
          ) : (
            <div className="market-stat">
              <div className="stat-header">
                <span className="stat-label">Borrow APY</span>
              </div>
              <span className="stat-value">{formatApy(pool.borrow_apy)}</span>
            </div>
          )}

          <div className="market-stat">
            <div className="stat-header">
              <span className="stat-label">Utilization</span>
            </div>
            <span className="stat-value">
              {formatUtilization(pool.utilization_pct)}
            </span>
          </div>

          <div className="market-stat">
            <div className="stat-header">
              <span className="stat-label">Available</span>
            </div>
            <span className="stat-value">
              {formatAmount(pool.available_liquidity, pool.decimals)}
            </span>
          </div>

          {mode === 'supply' ? (
            <div className="market-stat">
              <div className="stat-header">
                <span className="stat-label">Borrow APY</span>
              </div>
              <span className="stat-value">{formatApy(pool.borrow_apy)}</span>
            </div>
          ) : (
            <div className="market-stat">
              <div className="stat-header">
                <span className="stat-label">Supply APY</span>
              </div>
              <span className="stat-value positive">
                {formatApy(pool.supply_apy)}
              </span>
            </div>
          )}
        </div>

        {/* User's Wallet Balance (when connected and has balance) */}
        {walletAddress && availableBalance && (
          <div className="user-balance-row">
            <span className="balance-label">Your Balance:</span>
            <span className="balance-value">
              {availableBalance} {pool.symbol}
            </span>
          </div>
        )}

        {/* Supply Mode: User's Lend Position */}
        {mode === 'supply' && walletAddress && hasLendPosition && lendPosition && (
          <div className="user-position-box lend">
            <div className="position-row">
              <span className="position-label">Your Supply</span>
              <span className="position-value">
                {formatAmount(lendPosition.underlying_value, pool.decimals)}{' '}
                {pool.symbol}
              </span>
            </div>
            <div className="position-row">
              <span className="position-label">LP Tokens</span>
              <span className="position-value">
                {formatAmount(lendPosition.lp_tokens, pool.decimals)}
              </span>
            </div>
            {BigInt(lendPosition.unrealized_profit) > 0n && (
              <div className="position-row profit">
                <span className="position-label">Unrealized Profit</span>
                <span className="position-value positive">
                  +{formatAmount(lendPosition.unrealized_profit, pool.decimals)}{' '}
                  {pool.symbol}
                </span>
              </div>
            )}
          </div>
        )}

        {/* Borrow Mode: Collateral Options */}
        {mode === 'borrow' && pool.collateral_options && pool.collateral_options.length > 0 && (
          <div className="collateral-options">
            <span className="collateral-label">Accepted Collateral:</span>
            {pool.collateral_options.map(opt => (
              <div key={opt.token_id} className="collateral-chip">
                {COLLATERAL_ICONS[opt.token_name] && (
                  <img src={COLLATERAL_ICONS[opt.token_name]} alt={opt.token_name} className="collateral-icon" />
                )}
                <span>{opt.token_name}</span>
                <span className="collateral-threshold">
                  {(opt.liquidation_threshold / 10).toFixed(0)}% LTV
                </span>
              </div>
            ))}
          </div>
        )}

        {/* Borrow Mode: User's Borrow Position */}
        {mode === 'borrow' && walletAddress && hasBorrowPosition && borrowPosition && (
          <div
            className={`user-position-box borrow ${borrowPosition.health_status}`}
          >
            <div className="position-row">
              <span className="position-label">Your Borrow</span>
              <span className="position-value">
                {formatAmount(borrowPosition.borrowed_amount, pool.decimals)}{' '}
                {pool.symbol}
              </span>
            </div>
            <div className="position-row">
              <span className="position-label">Total Owed</span>
              <span className="position-value">
                {formatAmount(borrowPosition.total_owed, pool.decimals)}{' '}
                {pool.symbol}
              </span>
            </div>
            <div className="position-row">
              <span className="position-label">Collateral</span>
              <span className="position-value">
                {formatAmount(
                  borrowPosition.collateral_amount,
                  9
                )}{' '}
                {borrowPosition.collateral_name || 'ERG'}
              </span>
            </div>
            <div className="position-row">
              <span className="position-label">Health Factor</span>
              <span
                className={`position-value health-${borrowPosition.health_status}`}
              >
                {borrowPosition.health_factor.toFixed(2)}
                {borrowPosition.health_status === 'red' && (
                  <span className="health-warning" title="At risk of liquidation">
                    {' '}
                    !
                  </span>
                )}
              </span>
            </div>
          </div>
        )}

        {/* Supply Mode Actions */}
        {mode === 'supply' && (
          <div className="market-actions">
            <button
              className="action-btn primary"
              onClick={onLend}
              disabled={!walletAddress}
              title={
                !walletAddress ? 'Connect wallet first' : `Lend ${pool.symbol}`
              }
            >
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path d="M12 4v16m8-8H4" />
              </svg>
              Lend
            </button>
            <button
              className="action-btn secondary"
              onClick={onWithdraw}
              disabled={!walletAddress || !hasLendPosition}
              title={
                !walletAddress
                  ? 'Connect wallet first'
                  : !hasLendPosition
                    ? 'No position to withdraw'
                    : `Withdraw ${pool.symbol}`
              }
            >
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path d="M20 12H4" />
              </svg>
              Withdraw
            </button>
          </div>
        )}

        {/* Borrow Mode Actions */}
        {mode === 'borrow' && (
          <>
            <div className="market-actions">
              <button
                className="action-btn tertiary"
                onClick={onBorrow}
                disabled={!walletAddress}
                title={
                  !walletAddress
                    ? 'Connect wallet first'
                    : `Borrow ${pool.symbol}`
                }
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path d="M12 20V4M5 13l7 7 7-7" />
                </svg>
                Borrow
              </button>
            </div>
            {walletAddress && hasBorrowPosition && borrowPosition && (
              <div className="market-actions borrow-actions">
                <button
                  className="action-btn secondary"
                  onClick={() => onRepay(borrowPosition)}
                  title={`Repay ${pool.symbol} loan`}
                >
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <path d="M12 19V5M5 12l7-7 7 7" />
                  </svg>
                  Repay
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}

export default MarketCard
