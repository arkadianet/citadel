import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import { getProtocolActivity, type ProtocolInteraction } from '../api/protocolActivity'
import { getAmmPools, type AmmPool } from '../api/amm'
import './Dashboard.css'

interface DexyState {
  variant: string
  can_mint: boolean
  free_mint_available: number
  dexy_circulating: number
}

interface SigmaUsdState {
  sigrsv_price_nano: number
  sigusd_price_nano: number
  reserve_ratio_pct: number
  bank_erg_nano: number
  sigusd_circulating: number
  sigrsv_circulating: number
  bank_box_id: string
  oracle_erg_per_usd_nano: number
  oracle_box_id: string
  liabilities_nano: number
  equity_nano: number
  can_mint_sigusd: boolean
  can_mint_sigrsv: boolean
  can_redeem_sigusd: boolean
  can_redeem_sigrsv: boolean
  max_sigusd_mintable: number
  max_sigrsv_mintable: number
  max_sigrsv_redeemable: number
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

interface DashboardProps {
  isConnected: boolean
  ergUsd: number
  walletBalance?: WalletBalance | null
  sigmaUsdState?: SigmaUsdState | null
  explorerUrl: string
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

const PROTOCOL_ICONS: Record<string, string> = {
  SigmaUSD: '/icons/sigmausd.svg',
  DexyGold: '/icons/dexygold.svg',
  DexyUSD: '/icons/use.svg',
}

const TOKEN_ICONS: Record<string, string> = {
  SigUSD: '/icons/sigusd.svg',
  SigRSV: '/icons/sigrsv.svg',
  DexyGold: '/icons/dexygold.svg',
  USE: '/icons/use.svg',
  SPF: '/icons/spf.svg',
  NETA: '/icons/neta.svg',
  Ergopad: '/icons/ergopad.svg',
  Paideia: '/icons/paideia.svg',
  QUACKS: '/icons/quacks.svg',
  AHT: '/icons/aht.svg',
  Flux: '/icons/flux.svg',
  EXLE: '/icons/exle.svg',
}

const TOKEN_DECIMALS: Record<string, number> = {
  SigUSD: 2,
  SigRSV: 0,
  DexyGold: 0,
  USE: 3,
}

export function Dashboard({
  isConnected,
  ergUsd,
  walletBalance,
  sigmaUsdState,
}: DashboardProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [recentTxs, setRecentTxs] = useState<RecentTx[]>([])
  const [txsLoading, setTxsLoading] = useState(false)
  const [dexyGold, setDexyGold] = useState<DexyState | null>(null)
  const [dexyUsd, setDexyUsd] = useState<DexyState | null>(null)
  const [activity, setActivity] = useState<ProtocolInteraction[]>([])
  const [activityLoading, setActivityLoading] = useState(false)
  const [ammPools, setAmmPools] = useState<AmmPool[]>([])

  useEffect(() => {
    if (!isConnected) return
    getAmmPools().then(r => setAmmPools(r.pools)).catch(() => {})
  }, [isConnected])

  useEffect(() => {
    if (!isConnected) {
      setDexyGold(null)
      setDexyUsd(null)
      return
    }
    let cancelled = false
    const fetchDexy = async () => {
      try {
        const [gold, usd] = await Promise.all([
          invoke<DexyState>('get_dexy_state', { variant: 'gold' }),
          invoke<DexyState>('get_dexy_state', { variant: 'usd' }),
        ])
        if (!cancelled) {
          setDexyGold(gold)
          setDexyUsd(usd)
        }
      } catch (e) {
        console.error('Failed to fetch Dexy state:', e)
      }
    }
    fetchDexy()
    return () => { cancelled = true }
  }, [isConnected])

  useEffect(() => {
    if (!isConnected || !walletBalance) {
      setRecentTxs([])
      return
    }
    let cancelled = false
    setTxsLoading(true)
    invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 5 })
      .then(res => {
        if (!cancelled) setRecentTxs(res.transactions)
      })
      .catch(e => {
        console.error('Failed to fetch recent transactions:', e)
        if (!cancelled) setRecentTxs([])
      })
      .finally(() => {
        if (!cancelled) setTxsLoading(false)
      })
    return () => { cancelled = true }
  }, [isConnected, walletBalance])

  // Fetch protocol activity
  useEffect(() => {
    if (!isConnected) {
      setActivity([])
      return
    }
    let cancelled = false
    setActivityLoading(true)
    getProtocolActivity(5)
      .then(data => {
        if (!cancelled) setActivity(data)
      })
      .catch(e => {
        console.error('Failed to fetch protocol activity:', e)
        if (!cancelled) setActivity([])
      })
      .finally(() => {
        if (!cancelled) setActivityLoading(false)
      })
    return () => { cancelled = true }
  }, [isConnected])

  // Portfolio calculations
  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const sigusdBalance = walletBalance ? walletBalance.sigusd_amount / 100 : 0
  const sigrsvBalance = walletBalance?.sigrsv_amount ?? 0

  const ergValue = ergBalance * ergUsd
  const sigusdValue = sigusdBalance // 1 SigUSD ~ $1
  const sigrsvPrice = sigmaUsdState ? sigmaUsdState.sigrsv_price_nano / 1e9 : 0
  const sigrsvValue = sigrsvBalance * sigrsvPrice * ergUsd
  const totalValue = ergValue + sigusdValue + sigrsvValue

  // SigUSD DEX price divergence
  const SIGUSD_TOKEN_ID = '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
  const sigusdPool = ammPools
    .filter(p => p.pool_type === 'N2T' && p.token_y.token_id === SIGUSD_TOKEN_ID)
    .sort((a, b) => (b.erg_reserves ?? 0) - (a.erg_reserves ?? 0))[0] || null

  const dexErgUsd = sigusdPool && sigusdPool.erg_reserves
    ? (sigusdPool.token_y.amount / 100) / (sigusdPool.erg_reserves / 1e9)
    : null

  const divergencePct = dexErgUsd && ergUsd > 0
    ? ((dexErgUsd - ergUsd) / ergUsd) * 100
    : null

  const showDivergence = divergencePct !== null && Math.abs(divergencePct) > 3

  // RR recovery calculations (when RR < 400%)
  const rrRecovery = sigmaUsdState && sigmaUsdState.reserve_ratio_pct < 400 ? (() => {
    const rr = sigmaUsdState.reserve_ratio_pct
    const bankErg = sigmaUsdState.bank_erg_nano / 1e9
    const liabilities = sigmaUsdState.liabilities_nano / 1e9
    // Path 1: Mint SigRSV — each SigRSV minted adds ERG to the bank
    const ergNeeded = 4 * liabilities - bankErg
    const sigrsvPriceErg = sigmaUsdState.sigrsv_price_nano / 1e9
    const sigrsvNeeded = sigrsvPriceErg > 0 ? Math.ceil(ergNeeded / sigrsvPriceErg) : 0
    // Path 2: ERG price increase — if ERG price rises, bank value rises (liabilities stay same in USD)
    const priceIncreasePct = (400 / rr - 1) * 100
    // Path 3: SigUSD redemption — each SigUSD redeemed removes ~$1 liability and ~$1 ERG
    // After redeeming fraction f: new_rr = bank*(1-f) / (liab*(1-f)) ... actually more complex
    // Simplified: fraction = (4 - rr/100) / (4 - 1) = (4 - rr/100) / 3
    const redeemFraction = Math.min(1, Math.max(0, (4 - rr / 100) / 3))
    const sigusdToRedeem = Math.ceil((sigmaUsdState.sigusd_circulating / 100) * redeemFraction)
    return {
      ergNeeded: Math.max(0, ergNeeded),
      sigrsvNeeded: Math.max(0, sigrsvNeeded),
      sigrsvPriceErg,
      priceIncreasePct,
      redeemFraction,
      sigusdToRedeem,
    }
  })() : null

  return (
    <div className="dashboard">
      {/* Connect prompt */}
      {!isConnected && (
        <div className="connect-prompt">
          <p>Connect to an Ergo node to get started</p>
          <p className="text-muted">Click the settings icon in the top right</p>
        </div>
      )}

      {/* Portfolio Hero */}
      {isConnected && walletBalance && (
        <section className="portfolio-hero">
          <div className="portfolio-hero-left">
            <span className="portfolio-hero-label">Total Value</span>
            <span className="portfolio-hero-value">
              ${totalValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
            </span>
          </div>
          <div className="portfolio-hero-holdings">
            <div className="hero-holding orange">
              <div className="hero-holding-icon" style={{ background: 'rgba(249, 115, 22, 0.3)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.75rem', fontWeight: 700, color: '#fb923c' }}>
                E
              </div>
              <div className="hero-holding-info">
                <span className="hero-holding-name">ERG</span>
                <span className="hero-holding-amount">
                  {ergBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })}
                </span>
              </div>
              <span className="hero-holding-usd">${ergValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            </div>

            {sigusdBalance > 0 && (
              <div className="hero-holding emerald">
                <img src="/icons/sigusd.svg" alt="SigUSD" className="hero-holding-icon" />
                <div className="hero-holding-info">
                  <span className="hero-holding-name">SigUSD</span>
                  <span className="hero-holding-amount">
                    {sigusdBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
                  </span>
                </div>
                <span className="hero-holding-usd">${sigusdValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
              </div>
            )}

            {sigrsvBalance > 0 && (
              <div className="hero-holding blue">
                <img src="/icons/sigrsv.svg" alt="SigRSV" className="hero-holding-icon" />
                <div className="hero-holding-info">
                  <span className="hero-holding-name">SigRSV</span>
                  <span className="hero-holding-amount">{sigrsvBalance.toLocaleString()}</span>
                </div>
                <span className="hero-holding-usd">${sigrsvValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
              </div>
            )}

            {walletBalance.tokens
              .filter(t => t.name && t.token_id !== '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04' && t.token_id !== '003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0')
              .slice(0, 4)
              .map(t => (
                <div key={t.token_id} className="hero-holding slate">
                  <div className="hero-holding-icon" style={{ background: 'rgba(100, 116, 139, 0.3)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.6rem', fontWeight: 600, color: '#94a3b8' }}>
                    {(t.name ?? '?')[0].toUpperCase()}
                  </div>
                  <div className="hero-holding-info">
                    <span className="hero-holding-name">{t.name ?? t.token_id.slice(0, 8)}</span>
                    <span className="hero-holding-amount">
                      {(t.amount / Math.pow(10, t.decimals)).toLocaleString(undefined, { maximumFractionDigits: t.decimals })}
                    </span>
                  </div>
                </div>
              ))}
          </div>
        </section>
      )}

      {/* Protocol Status + Activity Feed (2-column) */}
      {isConnected && (sigmaUsdState || dexyGold) && (
        <div className="dashboard-protocols-row">
          {/* Left: Protocol Status */}
          <section className="protocol-status-section">
            <h3 className="dashboard-section-header">Protocol Status</h3>
            <div className="protocol-cards-stack">
              {sigmaUsdState && (
                <div className="protocol-card">
                  <div className="protocol-card-header">
                    <div className="protocol-card-title">
                      <img src="/icons/sigmausd.svg" alt="SigmaUSD" className="protocol-icon" />
                      <span className="protocol-card-name">SigmaUSD</span>
                    </div>
                    <span className="protocol-reserve-ratio">
                      RR {Math.round(sigmaUsdState.reserve_ratio_pct)}%
                    </span>
                  </div>
                  <div className="protocol-ops-grid">
                    <div className="protocol-op-cell">
                      <div className="protocol-op-cell-header">
                        <img src="/icons/sigusd.svg" alt="SigUSD" className="protocol-token-icon" />
                        <span className="protocol-op-token-name">SigUSD</span>
                      </div>
                      <div className="protocol-op-badges">
                        <span className={`protocol-op-badge ${sigmaUsdState.can_mint_sigusd ? 'open' : 'closed'}`}>Mint</span>
                        <span className={`protocol-op-badge ${sigmaUsdState.can_redeem_sigusd ? 'open' : 'closed'}`}>Redeem</span>
                      </div>
                      <span className="protocol-op-stat">{(sigmaUsdState.sigusd_circulating / Math.pow(10, TOKEN_DECIMALS.SigUSD)).toLocaleString()} circulating</span>
                    </div>
                    <div className="protocol-op-cell">
                      <div className="protocol-op-cell-header">
                        <img src="/icons/sigrsv.svg" alt="SigRSV" className="protocol-token-icon" />
                        <span className="protocol-op-token-name">SigRSV</span>
                      </div>
                      <div className="protocol-op-badges">
                        <span className={`protocol-op-badge ${sigmaUsdState.can_mint_sigrsv ? 'open' : 'closed'}`}>Mint</span>
                        <span className={`protocol-op-badge ${sigmaUsdState.can_redeem_sigrsv ? 'open' : 'closed'}`}>Redeem</span>
                      </div>
                      <span className="protocol-op-stat">{sigmaUsdState.sigrsv_circulating.toLocaleString()} circulating</span>
                    </div>
                  </div>
                  {showDivergence && dexErgUsd && divergencePct !== null && (
                    <div style={{
                      padding: '6px 10px',
                      marginTop: 6,
                      borderRadius: 6,
                      fontSize: 'var(--text-xs)',
                      background: Math.abs(divergencePct) > 10 ? 'rgba(239, 68, 68, 0.15)' : 'rgba(245, 158, 11, 0.15)',
                      color: Math.abs(divergencePct) > 10 ? 'var(--red-400)' : 'var(--amber-400)',
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      flexWrap: 'wrap',
                      gap: 4,
                    }}>
                      <span>
                        DEX ${dexErgUsd.toFixed(2)} vs Oracle ${ergUsd.toFixed(2)}
                        {' '}({divergencePct > 0 ? '+' : ''}{divergencePct.toFixed(1)}%)
                      </span>
                      <span style={{ fontWeight: 500 }}>
                        {sigmaUsdState.can_mint_sigusd
                          ? 'Arb available'
                          : `Arb blocked (RR ${Math.round(sigmaUsdState.reserve_ratio_pct)}%)`
                        }
                      </span>
                    </div>
                  )}
                  {rrRecovery && (
                    <div style={{
                      padding: '8px 10px',
                      marginTop: 6,
                      borderRadius: 6,
                      fontSize: 'var(--text-xs)',
                      background: 'rgba(99, 102, 241, 0.1)',
                      color: 'var(--indigo-300, #a5b4fc)',
                    }}>
                      <div style={{ fontWeight: 600, marginBottom: 4 }}>RR Recovery to 400%</div>
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 3, opacity: 0.9 }}>
                        <span>Mint ~{rrRecovery.sigrsvNeeded.toLocaleString()} SigRSV ({rrRecovery.ergNeeded.toLocaleString(undefined, { maximumFractionDigits: 1 })} ERG)</span>
                        <span>ERG price +{rrRecovery.priceIncreasePct.toFixed(1)}% (${(ergUsd * (1 + rrRecovery.priceIncreasePct / 100)).toFixed(2)})</span>
                        <span>Redeem ~{rrRecovery.sigusdToRedeem.toLocaleString()} SigUSD ({(rrRecovery.redeemFraction * 100).toFixed(1)}% of supply)</span>
                      </div>
                    </div>
                  )}
                </div>
              )}
              {dexyGold && dexyUsd && (
                <div className="protocol-card">
                  <div className="protocol-card-header">
                    <div className="protocol-card-title">
                      <img src="/icons/dexygold.svg" alt="Dexy" className="protocol-icon" />
                      <span className="protocol-card-name">Dexy</span>
                    </div>
                  </div>
                  <div className="protocol-ops-grid">
                    <div className="protocol-op-cell">
                      <div className="protocol-op-cell-header">
                        <img src="/icons/dexygold.svg" alt="DexyGold" className="protocol-token-icon" />
                        <span className="protocol-op-token-name">DexyGold</span>
                      </div>
                      <div className="protocol-op-badges">
                        <span className={`protocol-op-badge ${dexyGold.can_mint ? 'open' : 'closed'}`}>Mint</span>
                      </div>
                      <span className="protocol-op-stat">{dexyGold.free_mint_available.toLocaleString()} freemint available</span>
                      <span className="protocol-op-stat">{dexyGold.dexy_circulating.toLocaleString()} circulating</span>
                    </div>
                    <div className="protocol-op-cell">
                      <div className="protocol-op-cell-header">
                        <img src="/icons/use.svg" alt="USE" className="protocol-token-icon" />
                        <span className="protocol-op-token-name">USE</span>
                      </div>
                      <div className="protocol-op-badges">
                        <span className={`protocol-op-badge ${dexyUsd.can_mint ? 'open' : 'closed'}`}>Mint</span>
                      </div>
                      <span className="protocol-op-stat">{(dexyUsd.free_mint_available / Math.pow(10, TOKEN_DECIMALS.USE)).toLocaleString()} freemint available</span>
                      <span className="protocol-op-stat">{(dexyUsd.dexy_circulating / Math.pow(10, TOKEN_DECIMALS.USE)).toLocaleString()} circulating</span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </section>

          {/* Right: Protocol Activity Feed */}
          <section className="activity-feed-section">
            <h3 className="dashboard-section-header">Protocol Activity</h3>
            <div className="activity-feed-card">
              {activityLoading ? (
                <div className="activity-loading">
                  <div className="spinner-small" />
                  <span>Loading activity...</span>
                </div>
              ) : activity.length === 0 ? (
                <div className="activity-empty">No recent protocol activity</div>
              ) : (
                <div className="activity-list">
                  {activity.map((item, idx) => {
                    const op = item.operation
                    const opLabel = op === 'mint' ? 'Mint'
                      : op === 'redeem' ? 'Redeem'
                      : op === 'swap' ? 'Swap'
                      : op === 'lp_deposit' ? 'Add Liquidity'
                      : op === 'lp_redeem' ? 'Remove Liquidity'
                      : item.operation
                    const opClass = op === 'mint' || op === 'lp_deposit' ? 'mint'
                      : op === 'swap' ? 'swap' : 'redeem'
                    const ergAbs = Math.abs(item.erg_change_nano) / 1e9
                    const icon = TOKEN_ICONS[item.token] || PROTOCOL_ICONS[item.protocol]
                    return (
                      <div
                        key={`${item.tx_id}-${idx}`}
                        className="activity-row"
                        onClick={() => navigateToExplorer({ page: 'transaction', id: item.tx_id })}
                        role="button"
                        tabIndex={0}
                      >
                        <div className={`activity-op-icon ${opClass}`}>
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            {op === 'swap'
                              ? <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
                              : op === 'mint' || op === 'lp_deposit'
                                ? <path d="M12 19V5M5 12l7-7 7 7" />
                                : <path d="M12 5v14M5 12l7 7 7-7" />
                            }
                          </svg>
                        </div>
                        <div className="activity-info">
                          <div className="activity-label">
                            {icon && <img src={icon} alt="" className="activity-token-icon" />}
                            <span className="activity-op">{opLabel}</span>
                            <span className="activity-token">{item.token}</span>
                          </div>
                          <span className="activity-protocol">{item.protocol}</span>
                        </div>
                        <div className="activity-amounts">
                          {op === 'swap' && item.token_amount_change > 0 ? (() => {
                            // Swap direction: erg_change_nano > 0 means pool gained ERG = user paid ERG, received token
                            const userPaidErg = item.erg_change_nano > 0
                            const decimals = TOKEN_DECIMALS[item.token] ?? 0
                            const tokenAmt = decimals > 0
                              ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                              : item.token_amount_change.toLocaleString()
                            const ergAmt = ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })
                            return userPaidErg ? (<>
                              <span className="activity-token-amt positive">+{tokenAmt} {item.token}</span>
                              <span className="activity-erg-amt negative">-{ergAmt} ERG</span>
                            </>) : (<>
                              <span className="activity-token-amt negative">-{tokenAmt} {item.token}</span>
                              <span className="activity-erg-amt positive">+{ergAmt} ERG</span>
                            </>)
                          })() : (<>
                            {item.token_amount_change > 0 && (() => {
                              const decimals = TOKEN_DECIMALS[item.token] ?? 0
                              const amt = decimals > 0
                                ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                                : item.token_amount_change.toLocaleString()
                              const isPositive = op === 'mint' || op === 'lp_deposit'
                              return (
                                <span className={`activity-token-amt ${isPositive ? 'positive' : 'negative'}`}>
                                  {amt} {item.token}
                                </span>
                              )
                            })()}
                            {ergAbs > 0 && (
                              <span className="activity-erg-amt">
                                {ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })} ERG
                              </span>
                            )}
                          </>)}
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
          </section>
        </div>
      )}

      {/* Recent Wallet Transactions */}
      {isConnected && walletBalance && (
        <section className="recent-txs-section">
          <h3 className="dashboard-section-header">Your Recent Transactions</h3>
          {txsLoading ? (
            <div className="txs-loading">
              <div className="spinner-small" />
              <span>Loading transactions...</span>
            </div>
          ) : recentTxs.length === 0 ? (
            <div className="txs-empty">No recent transactions</div>
          ) : (
            <div className="txs-list">
              {recentTxs.map(tx => {
                const ergChange = tx.erg_change_nano / 1e9
                const isReceive = tx.erg_change_nano > 0
                return (
                  <div
                    key={tx.tx_id}
                    className="tx-row"
                    onClick={() => navigateToExplorer({ page: 'transaction', id: tx.tx_id })}
                    role="button"
                    tabIndex={0}
                  >
                    <div className={`tx-direction ${isReceive ? 'receive' : 'send'}`}>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        {isReceive
                          ? <path d="M12 5v14M5 12l7 7 7-7" />
                          : <path d="M12 19V5M5 12l7-7 7 7" />
                        }
                      </svg>
                    </div>
                    <div className="tx-info">
                      <span className="tx-id-short">{tx.tx_id.slice(0, 8)}...{tx.tx_id.slice(-6)}</span>
                      <span className="tx-time">{formatTimeAgo(tx.timestamp)}</span>
                    </div>
                    <div className="tx-amounts">
                      <span className={`tx-erg-change ${isReceive ? 'positive' : 'negative'}`}>
                        {isReceive ? '+' : ''}{ergChange.toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })} ERG
                      </span>
                      {tx.token_changes.slice(0, 2).map(tc => {
                        const amt = tc.amount / Math.pow(10, tc.decimals)
                        const isPos = tc.amount > 0
                        return (
                          <span key={tc.token_id} className={`tx-token-change ${isPos ? 'positive' : 'negative'}`}>
                            {isPos ? '+' : ''}{amt.toLocaleString(undefined, { maximumFractionDigits: tc.decimals })} {tc.name ?? tc.token_id.slice(0, 6)}
                          </span>
                        )
                      })}
                    </div>
                    <div className="tx-confirmations">
                      {tx.num_confirmations > 0
                        ? <span className="tx-confirmed">{tx.num_confirmations} conf</span>
                        : <span className="tx-pending">pending</span>
                      }
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </section>
      )}
    </div>
  )
}
