import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { DexyMintModal } from './DexyMintModal'
import { DexySwapModal } from './DexySwapModal'
import { getDexyActivity, type ProtocolInteraction } from '../api/protocolActivity'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import './DexyTab.css'

interface DexyState {
  variant: string
  bank_erg_nano: number
  dexy_in_bank: number
  bank_box_id: string
  dexy_token_id: string
  free_mint_available: number
  free_mint_reset_height: number
  current_height: number
  oracle_rate_nano: number
  oracle_box_id: string
  lp_erg_reserves: number
  lp_dexy_reserves: number
  lp_box_id: string
  lp_rate_nano: number
  can_mint: boolean
  rate_difference_pct: number
  dexy_circulating: number
}

interface WalletBalance {
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

interface DexyTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  ergUsdPrice?: number
  explorerUrl: string
}

type DexyVariant = 'gold' | 'usd'

const DEXY_TOKEN_IDS: Record<DexyVariant, string> = {
  gold: '6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad',
  usd: 'a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669',
}

const DEXY_TOKEN_ID_SET = new Set(Object.values(DEXY_TOKEN_IDS))

const TROY_OZ_IN_MG = 31103.5

interface TokenChange {
  token_id: string
  amount: number
  name: string | null
  decimals: number
}

interface RecentTx {
  tx_id: string
  inclusion_height: number
  num_confirmations: number
  timestamp: number
  erg_change_nano: number
  token_changes: TokenChange[]
}

const TOKEN_ICONS: Record<string, string> = {
  DexyGold: '/icons/dexygold.svg',
  USE: '/icons/use.svg',
}

const TOKEN_DECIMALS: Record<string, number> = {
  DexyGold: 0,
  USE: 3,
}

function formatTimeAgo(timestampMs: number): string {
  const now = Date.now()
  const diff = now - timestampMs
  const minutes = Math.floor(diff / 60000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(timestampMs).toLocaleDateString()
}

export function DexyTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  ergUsdPrice,
  explorerUrl,
}: DexyTabProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [goldState, setGoldState] = useState<DexyState | null>(null)
  const [usdState, setUsdState] = useState<DexyState | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [mintModalOpen, setMintModalOpen] = useState(false)
  const [swapModalOpen, setSwapModalOpen] = useState(false)
  const [selectedVariant, setSelectedVariant] = useState<DexyVariant>('gold')
  const [activity, setActivity] = useState<ProtocolInteraction[]>([])
  const [activityLoading, setActivityLoading] = useState(false)
  const [userTxs, setUserTxs] = useState<RecentTx[]>([])
  const [userTxsLoading, setUserTxsLoading] = useState(false)

  const fetchDexyState = useCallback(async (variant: DexyVariant) => {
    try {
      const state = await invoke<DexyState>('get_dexy_state', { variant })
      return state
    } catch (e) {
      console.error(`Failed to fetch Dexy ${variant} state:`, e)
      throw e
    }
  }, [])

  const fetchAllStates = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') {
      return
    }

    setLoading(true)
    setError(null)

    try {
      const [gold, usd] = await Promise.all([
        fetchDexyState('gold'),
        fetchDexyState('usd'),
      ])
      setGoldState(gold)
      setUsdState(usd)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier, fetchDexyState])

  const fetchDexyActivity = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setActivityLoading(true)
    try {
      const data = await getDexyActivity(10)
      setActivity(data)
    } catch (e) {
      console.error('Failed to fetch Dexy activity:', e)
      setActivity([])
    } finally {
      setActivityLoading(false)
    }
  }, [isConnected, capabilityTier])

  useEffect(() => {
    fetchAllStates()
    const interval = setInterval(fetchAllStates, 30000)
    return () => clearInterval(interval)
  }, [fetchAllStates])

  useEffect(() => {
    let cancelled = false
    if (!isConnected || capabilityTier === 'Basic') {
      setActivity([])
      return
    }
    setActivityLoading(true)
    getDexyActivity(10)
      .then(data => { if (!cancelled) setActivity(data) })
      .catch(e => {
        console.error('Failed to fetch Dexy activity:', e)
        if (!cancelled) setActivity([])
      })
      .finally(() => { if (!cancelled) setActivityLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, capabilityTier])

  // Fetch user's recent transactions, filtered to Dexy-related
  const fetchUserDexyTxs = useCallback(async () => {
    if (!isConnected || !walletBalance) {
      setUserTxs([])
      return
    }
    setUserTxsLoading(true)
    try {
      const res = await invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 20 })
      const dexyTxs = res.transactions.filter(tx =>
        tx.token_changes.some(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
      )
      setUserTxs(dexyTxs.slice(0, 10))
    } catch (e) {
      console.error('Failed to fetch user Dexy transactions:', e)
      setUserTxs([])
    } finally {
      setUserTxsLoading(false)
    }
  }, [isConnected, walletBalance])

  useEffect(() => {
    let cancelled = false
    if (!isConnected || !walletBalance) {
      setUserTxs([])
      return
    }
    setUserTxsLoading(true)
    invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 20 })
      .then(res => {
        if (cancelled) return
        const dexyTxs = res.transactions.filter(tx =>
          tx.token_changes.some(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
        )
        setUserTxs(dexyTxs.slice(0, 10))
      })
      .catch(e => {
        console.error('Failed to fetch user Dexy transactions:', e)
        if (!cancelled) setUserTxs([])
      })
      .finally(() => { if (!cancelled) setUserTxsLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, walletBalance])

  const openMintModal = (variant: DexyVariant) => {
    setSelectedVariant(variant)
    setMintModalOpen(true)
  }

  const openSwapModal = (variant: DexyVariant) => {
    setSelectedVariant(variant)
    setSwapModalOpen(true)
  }

  const getDexyBalance = (variant: DexyVariant): number => {
    if (!walletBalance) return 0
    const tokenId = DEXY_TOKEN_IDS[variant]
    const token = walletBalance.tokens.find(t => t.token_id === tokenId)
    return token?.amount || 0
  }

  // Holdings calculations
  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const ergUsd = ergBalance * (ergUsdPrice || 0)

  const goldBalance = getDexyBalance('gold')
  const goldUsdPerUnit = goldState && ergUsdPrice
    ? (goldState.oracle_rate_nano / 1e9) * ergUsdPrice
    : 0
  const goldUsd = goldBalance * goldUsdPerUnit

  const useBalance = getDexyBalance('usd') / 1e3 // USE has 3 decimals
  const useUsdPerUnit = usdState && ergUsdPrice
    ? (usdState.oracle_rate_nano / 1e9) * 1e3 * ergUsdPrice // rate is per raw unit, 1e3 raw = 1 USE
    : 0
  const useUsd = useBalance * useUsdPerUnit

  const totalHoldingsUsd = ergUsd + goldUsd + useUsd

  if (!isConnected) {
    return (
      <div className="dexy-tab">
        <div className="empty-state">
          <p>Connect to a node first</p>
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="dexy-tab">
        <div className="message error">
          Dexy requires an indexed node with extraIndex enabled.
        </div>
      </div>
    )
  }

  if (loading && !goldState && !usdState) {
    return (
      <div className="dexy-tab">
        <div className="empty-state">
          <div className="spinner" />
          <p>Loading Dexy protocol state...</p>
        </div>
      </div>
    )
  }

  if (error && !goldState && !usdState) {
    return (
      <div className="dexy-tab">
        <div className="message error">{error}</div>
      </div>
    )
  }

  return (
    <div className="dexy-tab">
      {/* Protocol Header */}
      <div className="dexy-header">
        <div className="dexy-header-row">
          <div className="dexy-icon-stack">
            <span className="icon-gold">
              <img src="/icons/dexygold.svg" alt="DexyGold" />
            </span>
            <span className="icon-usd">
              <img src="/icons/use.svg" alt="USE" />
            </span>
          </div>
          <div>
            <h2>Dexy Protocol</h2>
            <p className="dexy-description">Oracle-pegged stablecoins with LP dynamics</p>
          </div>
        </div>
      </div>

      {/* Protocol Info Bar */}
      <div className="dexy-info-bar">
        <div className="dexy-info-item">
          <span className="dexy-info-label">Variants:</span>
          <span className="dexy-info-value">DexyGold, USE</span>
        </div>
        <div className="dexy-info-divider" />
        <div className="dexy-info-item">
          <span className="dexy-info-label">Actions:</span>
          <span className="dexy-info-value">Mint, LP Swap</span>
        </div>
      </div>

      {/* Asset Cards */}
      <div className="token-cards-grid">
        <DexyAssetCard
          state={goldState}
          variant="gold"
          tokenName="DexyGold"
          decimals={0}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          onMint={() => openMintModal('gold')}
          onSwap={() => openSwapModal('gold')}
          ergUsdPrice={ergUsdPrice}
        />
        <DexyAssetCard
          state={usdState}
          variant="usd"
          tokenName="USE"
          decimals={3}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          onMint={() => openMintModal('usd')}
          onSwap={() => openSwapModal('usd')}
        />
      </div>

      {/* Your Holdings */}
      {walletAddress && walletBalance ? (
        <div className="dexy-holdings-section">
          <div className="dexy-holdings-header">
            <h3>Your Holdings</h3>
            {ergUsdPrice && (
              <span className="dexy-holdings-total">Total: ${totalHoldingsUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            )}
          </div>
          <div className="dexy-holdings-grid">
            <div className="dexy-holding-card orange">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon" style={{ background: 'rgba(249, 115, 22, 0.3)', color: '#fb923c' }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" width="14" height="14">
                    <path d="M20.24 12.24a6 6 0 0 0-8.49-8.49L5 10.5V19h8.5z" />
                  </svg>
                </div>
                <span className="dexy-holding-name">ERG</span>
              </div>
              <div className="dexy-holding-amount">{ergBalance.toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${ergUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
            <div className="dexy-holding-card amber">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon-wrap gold">
                  <img src="/icons/dexygold.svg" alt="DexyGold" />
                </div>
                <span className="dexy-holding-name">DexyGold</span>
              </div>
              <div className="dexy-holding-amount">{goldBalance.toLocaleString()}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${goldUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
            <div className="dexy-holding-card emerald">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon-wrap usd">
                  <img src="/icons/use.svg" alt="USE" />
                </div>
                <span className="dexy-holding-name">USE</span>
              </div>
              <div className="dexy-holding-amount">{useBalance.toLocaleString(undefined, { minimumFractionDigits: 3, maximumFractionDigits: 3 })}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${useUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
          </div>
        </div>
      ) : !walletAddress && (
        <div className="dexy-wallet-section">
          <div className="wallet-notice">
            <div className="wallet-notice-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="5" width="20" height="14" rx="2" />
                <path d="M2 10h20" />
              </svg>
            </div>
            <h3>Wallet Not Connected</h3>
            <p>Connect your wallet using the button in the header to mint Dexy tokens</p>
          </div>
        </div>
      )}

      {/* Activity Feeds — side by side */}
      <div className="dexy-activity-grid">
        {/* Your Dexy Activity */}
        <div className="dexy-activity-section">
          <h3 className="dexy-section-header">Your Dexy Activity</h3>
          <div className="dexy-activity-card">
            {!walletAddress ? (
              <div className="dexy-activity-empty">Connect wallet to see your activity</div>
            ) : userTxsLoading ? (
              <div className="dexy-activity-loading">
                <div className="spinner-small" />
                <span>Loading...</span>
              </div>
            ) : userTxs.length === 0 ? (
              <div className="dexy-activity-empty">No recent Dexy transactions</div>
            ) : (
              <div className="dexy-activity-list">
                {userTxs.map(tx => {
                  const dexyChanges = tx.token_changes.filter(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
                  const ergChange = tx.erg_change_nano / 1e9
                  const isReceive = tx.erg_change_nano > 0
                  return (
                    <div
                      key={tx.tx_id}
                      className="dexy-activity-row"
                      onClick={() => navigateToExplorer({ page: 'transaction', id: tx.tx_id })}
                      role="button"
                      tabIndex={0}
                    >
                      <div className={`activity-op-icon ${isReceive ? 'mint' : 'redeem'}`}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                          {isReceive
                            ? <path d="M12 5v14M5 12l7 7 7-7" />
                            : <path d="M12 19V5M5 12l7-7 7 7" />
                          }
                        </svg>
                      </div>
                      <div className="activity-info">
                        <span className="activity-op">{tx.tx_id.slice(0, 8)}...{tx.tx_id.slice(-6)}</span>
                      </div>
                      <div className="activity-amounts">
                        {dexyChanges.map(tc => {
                          const amt = tc.amount / Math.pow(10, tc.decimals)
                          const isPos = tc.amount > 0
                          return (
                            <span key={tc.token_id} className={`activity-token-amt ${isPos ? 'positive' : 'negative'}`}>
                              {isPos ? '+' : ''}{amt.toLocaleString(undefined, { maximumFractionDigits: tc.decimals })} {tc.name ?? tc.token_id.slice(0, 6)}
                            </span>
                          )
                        })}
                        <span className="activity-erg-amt">
                          {isReceive ? '+' : ''}{ergChange.toLocaleString(undefined, { maximumFractionDigits: 4 })} ERG
                        </span>
                      </div>
                      <span className="activity-time">
                        {tx.timestamp > 0 ? formatTimeAgo(tx.timestamp) : `#${tx.inclusion_height}`}
                      </span>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </div>

        {/* Recent Protocol Activity */}
        <div className="dexy-activity-section">
          <h3 className="dexy-section-header">Recent Protocol Activity</h3>
          <div className="dexy-activity-card">
            {activityLoading ? (
              <div className="dexy-activity-loading">
                <div className="spinner-small" />
                <span>Loading activity...</span>
              </div>
            ) : activity.length === 0 ? (
              <div className="dexy-activity-empty">No recent Dexy protocol activity</div>
            ) : (
              <div className="dexy-activity-list">
                {activity.map((item, idx) => {
                  const isMint = item.operation === 'mint'
                  const ergAbs = Math.abs(item.erg_change_nano) / 1e9
                  const icon = TOKEN_ICONS[item.token]
                  return (
                    <div
                      key={`${item.tx_id}-${idx}`}
                      className="dexy-activity-row"
                      onClick={() => navigateToExplorer({ page: 'transaction', id: item.tx_id })}
                      role="button"
                      tabIndex={0}
                    >
                      <div className={`activity-op-icon ${isMint ? 'mint' : 'redeem'}`}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                          {isMint
                            ? <path d="M12 19V5M5 12l7-7 7 7" />
                            : <path d="M12 5v14M5 12l7 7 7-7" />
                          }
                        </svg>
                      </div>
                      <div className="activity-info">
                        <div className="activity-label">
                          {icon && (
                            <span className={`activity-token-icon-wrap ${item.token === 'DexyGold' ? 'gold' : 'usd'}`}>
                              <img src={icon} alt="" />
                            </span>
                          )}
                          <span className="activity-op">{isMint ? 'Mint' : 'Redeem'}</span>
                          <span className="activity-token">{item.token}</span>
                        </div>
                        <span className="activity-protocol">{item.protocol}</span>
                      </div>
                      <div className="activity-amounts">
                        {item.token_amount_change > 0 && (() => {
                          const decimals = TOKEN_DECIMALS[item.token] ?? 0
                          const amt = decimals > 0
                            ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                            : item.token_amount_change.toLocaleString()
                          return (
                            <span className={`activity-token-amt ${isMint ? 'positive' : 'negative'}`}>
                              {isMint ? '+' : '-'}{amt} {item.token}
                            </span>
                          )
                        })()}
                        {ergAbs > 0 && (
                          <span className="activity-erg-amt">
                            {ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })} ERG
                          </span>
                        )}
                      </div>
                      <span className="activity-time">
                        {item.timestamp > 0 ? formatTimeAgo(item.timestamp) : `#${item.height}`}
                      </span>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Mint Modal */}
      {mintModalOpen && walletAddress && walletBalance && (
        <DexyMintModal
          isOpen={mintModalOpen}
          onClose={() => setMintModalOpen(false)}
          variant={selectedVariant}
          state={selectedVariant === 'gold' ? goldState : usdState}
          walletAddress={walletAddress}
          ergBalance={walletBalance.erg_nano}
          explorerUrl={explorerUrl}
          onSuccess={() => {
            fetchAllStates()
            fetchDexyActivity()
            fetchUserDexyTxs()
          }}
        />
      )}

      {/* Swap Modal */}
      {swapModalOpen && walletAddress && walletBalance && (
        <DexySwapModal
          isOpen={swapModalOpen}
          onClose={() => setSwapModalOpen(false)}
          variant={selectedVariant}
          state={selectedVariant === 'gold' ? goldState : usdState}
          walletAddress={walletAddress}
          ergBalance={walletBalance.erg_nano}
          dexyBalance={getDexyBalance(selectedVariant)}
          explorerUrl={explorerUrl}
          onSuccess={() => {
            fetchAllStates()
            fetchDexyActivity()
            fetchUserDexyTxs()
          }}
        />
      )}
    </div>
  )
}

// Asset card matching SigmaUSD's token-card structure
function DexyAssetCard({
  state,
  variant,
  tokenName,
  decimals,
  walletAddress,
  walletBalance,
  onMint,
  onSwap,
  ergUsdPrice,
}: {
  state: DexyState | null
  variant: DexyVariant
  tokenName: string
  decimals: number
  walletAddress: string | null
  walletBalance: WalletBalance | null
  onMint: () => void
  onSwap: () => void
  ergUsdPrice?: number
}) {
  const formatRate = (rateNano: number) => {
    const tokenMultiplier = Math.pow(10, decimals)
    const ergPerDisplayUnit = (rateNano / 1e9) * tokenMultiplier
    return ergPerDisplayUnit.toFixed(4)
  }

  const formatAmount = (amount: number, dec: number) => {
    const divisor = Math.pow(10, dec)
    return (amount / divisor).toLocaleString(undefined, {
      minimumFractionDigits: dec,
      maximumFractionDigits: dec,
    })
  }

  const description = variant === 'gold'
    ? '1 DexyGold = 1mg of gold'
    : 'USD-pegged stablecoin'

  const goldUsdPerOz = variant === 'gold' && state && ergUsdPrice
    ? (state.oracle_rate_nano / 1e9) * TROY_OZ_IN_MG * ergUsdPrice
    : null

  const iconSrc = variant === 'gold' ? '/icons/dexygold.svg' : '/icons/use.svg'
  const iconAlt = variant === 'gold' ? 'DexyGold' : 'USE'
  const colorClass = variant === 'gold' ? 'amber' : 'emerald'

  const userBalance = walletBalance?.tokens.find(t => t.token_id === DEXY_TOKEN_IDS[variant])?.amount || 0

  if (!state) {
    return (
      <div className={`token-card ${colorClass}`}>
        <div className="token-card-header">
          <div className="token-header-content">
            <div className="token-header-left">
              <div className={`dexy-token-icon-wrap ${variant}`}>
                <img src={iconSrc} alt={iconAlt} />
              </div>
              <div className="token-info">
                <h3>{tokenName}</h3>
                <p>{description}</p>
              </div>
            </div>
            <span className="token-ticker">{tokenName.toUpperCase()}</span>
          </div>
        </div>
        <div className="token-card-body">
          <div className="asset-loading">
            <div className="spinner-small" />
            <span>Loading...</span>
          </div>
        </div>
      </div>
    )
  }

  const canMint = walletAddress && state.can_mint

  return (
    <div className={`token-card ${colorClass}`}>
      <div className="token-card-header">
        <div className="token-header-content">
          <div className="token-header-left">
            <div className={`dexy-token-icon-wrap ${variant}`}>
              <img src={iconSrc} alt={iconAlt} />
            </div>
            <div className="token-info">
              <h3>{tokenName}</h3>
              <p>{description}</p>
            </div>
          </div>
          <span className="token-ticker">{tokenName.toUpperCase()}</span>
        </div>
      </div>

      <div className="token-card-body">
        <div className="token-stats">
          <div className="token-stat">
            <span className="token-stat-label">Oracle Rate</span>
            <span className="token-stat-value">{formatRate(state.oracle_rate_nano)} ERG</span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">LP Rate</span>
            <span className="token-stat-value">{formatRate(state.lp_rate_nano)} ERG</span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">Rate Diff</span>
            <span className={`token-stat-value ${state.rate_difference_pct > 0 ? 'positive' : state.rate_difference_pct < 0 ? 'negative' : ''}`}>
              {state.rate_difference_pct > 0 ? '+' : ''}{state.rate_difference_pct.toFixed(2)}%
            </span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">Circulating</span>
            <span className="token-stat-value">{formatAmount(state.dexy_circulating, decimals)}</span>
          </div>
          {goldUsdPerOz !== null && (
            <div className="token-stat">
              <span className="token-stat-label">Gold (USD/oz)</span>
              <span className="token-stat-value">${goldUsdPerOz.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            </div>
          )}
        </div>

        {walletAddress && userBalance > 0 && (
          <div className="wallet-balance-box">
            <div className="wallet-balance-row">
              <span className="wallet-balance-label">Your Balance</span>
              <div className="wallet-balance-value">
                <span className="wallet-balance-amount">{formatAmount(userBalance, decimals)}</span>
                <span className="wallet-balance-ticker">{tokenName.toUpperCase()}</span>
              </div>
            </div>
          </div>
        )}

        <div className="token-actions">
          <button
            className={`action-btn ${canMint ? `primary ${colorClass}` : ''}`}
            disabled={!canMint}
            onClick={onMint}
            title={!walletAddress ? 'Connect wallet first' : !state.can_mint ? 'Minting unavailable' : `Mint ${tokenName}`}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 4v16m8-8H4" />
            </svg>
            Mint
          </button>
          <button
            className={`action-btn ${walletAddress ? 'secondary' : ''}`}
            disabled={!walletAddress}
            onClick={onSwap}
            title={!walletAddress ? 'Connect wallet first' : `Swap via LP pool`}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
            </svg>
            Swap
          </button>
        </div>

        <div className="status-badges">
          <span className={`status-badge ${state.can_mint ? 'available' : 'unavailable'}`}>
            <span className="dot" />
            Mint {state.can_mint ? 'Available' : 'Unavailable'}
          </span>
          {(() => {
            // Compare effective cost per token: FreeMint (oracle rate) vs LP Swap (lp rate + 0.3% fee)
            const mintRate = state.oracle_rate_nano
            const swapEffective = state.lp_rate_nano * 1.003
            const mintBetter = state.can_mint && mintRate < swapEffective
            const savingPct = Math.abs(mintRate - swapEffective) / Math.max(mintRate, swapEffective) * 100
            return (
              <span className={`status-badge best-path ${mintBetter ? 'mint-best' : 'swap-best'}`}>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                Best: {mintBetter ? 'Mint' : 'LP Swap'}
                {savingPct > 0.1 && <span className="saving-pct">({savingPct.toFixed(1)}% cheaper)</span>}
              </span>
            )
          })()}
        </div>
      </div>
    </div>
  )
}
