import { useState, useEffect, type CSSProperties } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import { getProtocolActivity, type ProtocolInteraction } from '../api/protocolActivity'
import { getAmmPools, type AmmPool } from '../api/amm'
import { StatTile, EmptyState } from './ui'
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

type View = 'home' | 'sigmausd' | 'dexy' | 'lending' | 'dex' | 'hodlcoin' | 'bonds' | 'timelocks' | 'router' | 'arb-scanner' | 'explorer' | 'wallet'

interface DashboardProps {
  isConnected: boolean
  ergUsd: number
  walletBalance?: WalletBalance | null
  sigmaUsdState?: SigmaUsdState | null
  explorerUrl: string
  blockHeight?: number
  onNavigate?: (view: View) => void
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

function rrTone(rr: number): 'healthy' | 'caution' | 'critical' {
  if (rr >= 400) return 'healthy'
  if (rr >= 200) return 'caution'
  return 'critical'
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

const SIGUSD_TOKEN_ID = '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
const SIGRSV_TOKEN_ID = '003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0'

const PROTOCOL_GRID: Array<{
  id: View
  name: string
  desc: string
  icon?: string
  accent: string
}> = [
  { id: 'sigmausd', name: 'SigmaUSD', desc: 'AgeUSD stablecoin', icon: '/icons/sigmausd.svg', accent: '#34d399' },
  { id: 'dexy', name: 'Dexy', desc: 'Oracle-pegged assets', icon: '/icons/dexygold.svg', accent: '#fbbf24' },
  { id: 'lending', name: 'Lending', desc: 'Duckpools markets', icon: '/icons/quacks.svg', accent: '#60a5fa' },
  { id: 'dex', name: 'DEX', desc: 'AMM swaps', icon: '/icons/spf.svg', accent: '#a78bfa' },
  { id: 'hodlcoin', name: 'HodlCoin', desc: 'Hold & earn', icon: '/icons/hodlerg3.svg', accent: '#fb923c' },
  { id: 'bonds', name: 'Bonds', desc: 'SigmaFi P2P', accent: '#f472b6' },
  { id: 'timelocks', name: 'Timelocks', desc: 'MewLock vaults', icon: '/icons/mew.png', accent: '#94a3b8' },
]

export function Dashboard({
  isConnected,
  ergUsd,
  walletBalance,
  sigmaUsdState,
  blockHeight,
  onNavigate,
}: DashboardProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [recentTxs, setRecentTxs] = useState<RecentTx[]>([])
  const [txsLoading, setTxsLoading] = useState(false)
  const [dexyGold, setDexyGold] = useState<DexyState | null>(null)
  const [dexyUsd, setDexyUsd] = useState<DexyState | null>(null)
  const [activity, setActivity] = useState<ProtocolInteraction[]>([])
  const [activityLoading, setActivityLoading] = useState(false)
  const [ammPools, setAmmPools] = useState<AmmPool[]>([])
  const [feedTab, setFeedTab] = useState<'activity' | 'wallet'>('activity')

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

  useEffect(() => {
    if (!isConnected) {
      setActivity([])
      return
    }
    let cancelled = false
    setActivityLoading(true)
    getProtocolActivity(100, 24 * 60 * 60)
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

  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const sigusdBalance = walletBalance ? walletBalance.sigusd_amount / 100 : 0
  const sigrsvBalance = walletBalance?.sigrsv_amount ?? 0

  const ergValue = ergBalance * ergUsd
  const sigusdValue = sigusdBalance
  const sigrsvPrice = sigmaUsdState ? sigmaUsdState.sigrsv_price_nano / 1e9 : 0
  const sigrsvValue = sigrsvBalance * sigrsvPrice * ergUsd
  const totalValue = ergValue + sigusdValue + sigrsvValue

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

  const rrRecovery = sigmaUsdState && sigmaUsdState.reserve_ratio_pct < 400 ? (() => {
    const rr = sigmaUsdState.reserve_ratio_pct
    const bankErg = sigmaUsdState.bank_erg_nano / 1e9
    const liabilities = sigmaUsdState.liabilities_nano / 1e9
    const ergNeeded = 4 * liabilities - bankErg
    const sigrsvPriceErg = sigmaUsdState.sigrsv_price_nano / 1e9
    const sigrsvNeeded = sigrsvPriceErg > 0 ? Math.ceil(ergNeeded / sigrsvPriceErg) : 0
    const priceIncreasePct = (400 / rr - 1) * 100
    const redeemFraction = Math.min(1, Math.max(0, (4 - rr / 100) / 3))
    const sigusdToRedeem = Math.ceil((sigmaUsdState.sigusd_circulating / 100) * redeemFraction)
    return {
      ergNeeded: Math.max(0, ergNeeded),
      sigrsvNeeded: Math.max(0, sigrsvNeeded),
      priceIncreasePct,
      redeemFraction,
      sigusdToRedeem,
    }
  })() : null

  const otherTokens = walletBalance
    ? walletBalance.tokens
        .filter(t => t.name && t.token_id !== SIGUSD_TOKEN_ID && t.token_id !== SIGRSV_TOKEN_ID)
        .slice(0, 5)
    : []

  const hasAlerts = showDivergence || !!rrRecovery

  const renderActivityRows = () => {
    if (activityLoading) {
      return (
        <div className="dash-feed-state">
          <div className="spinner-small" />
          <span>Loading activity…</span>
        </div>
      )
    }
    if (activity.length === 0) {
      return <div className="dash-feed-state">No recent protocol activity</div>
    }
    return (
      <div className="dash-feed-list">
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
            <button
              key={`${item.tx_id}-${idx}`}
              type="button"
              className="dash-feed-row"
              onClick={() => navigateToExplorer({ page: 'transaction', id: item.tx_id })}
            >
              <div className={`dash-feed-icon ${opClass}`} aria-hidden>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                  {op === 'swap'
                    ? <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
                    : op === 'mint' || op === 'lp_deposit'
                      ? <path d="M12 19V5M5 12l7-7 7 7" />
                      : <path d="M12 5v14M5 12l7 7 7-7" />
                  }
                </svg>
              </div>
              <div className="dash-feed-info">
                <div className="dash-feed-label">
                  {icon && <img src={icon} alt="" />}
                  <span className="dash-feed-op">{opLabel}</span>
                  <span className="dash-feed-token">{item.token}</span>
                </div>
                <span className="dash-feed-protocol">{item.protocol}</span>
              </div>
              <div className="dash-feed-amounts">
                {op === 'swap' && item.token_amount_change > 0 ? (() => {
                  const userPaidErg = item.erg_change_nano > 0
                  const decimals = TOKEN_DECIMALS[item.token] ?? 0
                  const tokenAmt = decimals > 0
                    ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                    : item.token_amount_change.toLocaleString()
                  const ergAmt = ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })
                  return userPaidErg ? (
                    <>
                      <span className="amt positive">+{tokenAmt} {item.token}</span>
                      <span className="amt negative">-{ergAmt} ERG</span>
                    </>
                  ) : (
                    <>
                      <span className="amt negative">-{tokenAmt} {item.token}</span>
                      <span className="amt positive">+{ergAmt} ERG</span>
                    </>
                  )
                })() : (
                  <>
                    {item.token_amount_change > 0 && (() => {
                      const decimals = TOKEN_DECIMALS[item.token] ?? 0
                      const amt = decimals > 0
                        ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                        : item.token_amount_change.toLocaleString()
                      const isPositive = op === 'mint' || op === 'lp_deposit'
                      return (
                        <span className={`amt ${isPositive ? 'positive' : 'negative'}`}>
                          {amt} {item.token}
                        </span>
                      )
                    })()}
                    {ergAbs > 0 && (
                      <span className="amt muted">
                        {ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })} ERG
                      </span>
                    )}
                  </>
                )}
              </div>
              <span className="dash-feed-time">
                {item.timestamp > 0 ? formatTimeAgo(item.timestamp) : `#${item.height}`}
              </span>
            </button>
          )
        })}
      </div>
    )
  }

  const renderWalletRows = () => {
    if (txsLoading) {
      return (
        <div className="dash-feed-state">
          <div className="spinner-small" />
          <span>Loading transactions…</span>
        </div>
      )
    }
    if (recentTxs.length === 0) {
      return <div className="dash-feed-state">No recent transactions</div>
    }
    return (
      <div className="dash-feed-list">
        {recentTxs.map(tx => {
          const ergChange = tx.erg_change_nano / 1e9
          const isReceive = tx.erg_change_nano > 0
          return (
            <button
              key={tx.tx_id}
              type="button"
              className="dash-feed-row"
              onClick={() => navigateToExplorer({ page: 'transaction', id: tx.tx_id })}
            >
              <div className={`dash-feed-icon ${isReceive ? 'mint' : 'send'}`} aria-hidden>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  {isReceive
                    ? <path d="M12 5v14M5 12l7 7 7-7" />
                    : <path d="M12 19V5M5 12l7-7 7 7" />
                  }
                </svg>
              </div>
              <div className="dash-feed-info">
                <span className="dash-feed-op mono">{tx.tx_id.slice(0, 8)}…{tx.tx_id.slice(-6)}</span>
                <span className="dash-feed-protocol">{formatTimeAgo(tx.timestamp)}</span>
              </div>
              <div className="dash-feed-amounts">
                <span className={`amt ${isReceive ? 'positive' : 'negative'}`}>
                  {isReceive ? '+' : ''}{ergChange.toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })} ERG
                </span>
                {tx.token_changes.slice(0, 2).map(tc => {
                  const amt = tc.amount / Math.pow(10, tc.decimals)
                  const isPos = tc.amount > 0
                  return (
                    <span key={tc.token_id} className={`amt ${isPos ? 'positive' : 'negative'}`}>
                      {isPos ? '+' : ''}{amt.toLocaleString(undefined, { maximumFractionDigits: tc.decimals })}{' '}
                      {tc.name ?? tc.token_id.slice(0, 6)}
                    </span>
                  )
                })}
              </div>
              <span className="dash-feed-time">
                {tx.num_confirmations > 0
                  ? <span className="dash-conf">{tx.num_confirmations} conf</span>
                  : <span className="dash-pending">pending</span>
                }
              </span>
            </button>
          )
        })}
      </div>
    )
  }

  return (
    <div className="dashboard">
      <header className="dash-header">
        <h1 className="dash-title">Overview</h1>
        {isConnected && (
          <div className="dash-header-meta">
            {blockHeight != null && (
              <span className="dash-meta-chip">
                <span className="dash-live-dot" aria-hidden />
                Block {blockHeight.toLocaleString()}
              </span>
            )}
            {ergUsd > 0 && (
              <span className="dash-meta-chip mono">ERG ${ergUsd.toFixed(2)}</span>
            )}
          </div>
        )}
      </header>

      {!isConnected && (
        <div className="dash-empty-wrap">
          <EmptyState
            title="Connect to get started"
            description="Connect to an Ergo node from settings in the top right to load your portfolio and protocol status."
          />
        </div>
      )}

      {isConnected && (
        <>
          <div className="dash-top">
            {walletBalance && (
              <section className="dash-portfolio">
                <div className="dash-portfolio-main">
                  <span className="dash-label">Total portfolio</span>
                  <div className="dash-portfolio-value">
                    ${totalValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                  </div>
                </div>
                <div className="dash-holdings">
                  <div className="dash-holding">
                    <div className="dash-holding-icon dash-holding-icon--erg" aria-hidden>Σ</div>
                    <div className="dash-holding-body">
                      <span className="dash-holding-name">ERG</span>
                      <span className="dash-holding-amt mono">
                        {ergBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })}
                      </span>
                    </div>
                    <span className="dash-holding-usd mono">
                      ${ergValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                    </span>
                  </div>

                  {sigusdBalance > 0 && (
                    <div className="dash-holding">
                      <img src="/icons/sigusd.svg" alt="" className="dash-holding-icon" />
                      <div className="dash-holding-body">
                        <span className="dash-holding-name">SigUSD</span>
                        <span className="dash-holding-amt mono">
                          {sigusdBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
                        </span>
                      </div>
                      <span className="dash-holding-usd mono">
                        ${sigusdValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                      </span>
                    </div>
                  )}

                  {sigrsvBalance > 0 && (
                    <div className="dash-holding">
                      <img src="/icons/sigrsv.svg" alt="" className="dash-holding-icon" />
                      <div className="dash-holding-body">
                        <span className="dash-holding-name">SigRSV</span>
                        <span className="dash-holding-amt mono">{sigrsvBalance.toLocaleString()}</span>
                      </div>
                      <span className="dash-holding-usd mono">
                        ${sigrsvValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                      </span>
                    </div>
                  )}

                  {otherTokens.map(t => (
                    <div key={t.token_id} className="dash-holding">
                      <div className="dash-holding-icon dash-holding-icon--token" aria-hidden>
                        {(t.name ?? '?')[0].toUpperCase()}
                      </div>
                      <div className="dash-holding-body">
                        <span className="dash-holding-name">{t.name ?? t.token_id.slice(0, 8)}</span>
                        <span className="dash-holding-amt mono">
                          {(t.amount / Math.pow(10, t.decimals)).toLocaleString(undefined, {
                            maximumFractionDigits: t.decimals,
                          })}
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              </section>
            )}

            <section className="dash-stats">
              <StatTile
                label="ERG Price"
                value={ergUsd > 0 ? `$${ergUsd.toFixed(2)}` : '—'}
              />
              <StatTile
                label="SigUSD Rate"
                value={sigmaUsdState ? `${(sigmaUsdState.sigusd_price_nano / 1e9).toFixed(4)} ERG` : '—'}
              />
              <StatTile
                label="Reserve Ratio"
                value={sigmaUsdState ? `${Math.round(sigmaUsdState.reserve_ratio_pct)}%` : '—'}
                change={
                  sigmaUsdState
                    ? (sigmaUsdState.reserve_ratio_pct >= 400
                      ? 'Healthy'
                      : sigmaUsdState.reserve_ratio_pct >= 200
                        ? 'Caution'
                        : 'Critical')
                    : undefined
                }
                changeDirection={
                  sigmaUsdState
                    ? (sigmaUsdState.reserve_ratio_pct >= 400
                      ? 'up'
                      : sigmaUsdState.reserve_ratio_pct >= 200
                        ? 'stable'
                        : 'down')
                    : undefined
                }
              />
              <StatTile
                label="Block Height"
                value={blockHeight ? blockHeight.toLocaleString() : '—'}
              />
            </section>

            {hasAlerts && (
              <section className="dash-alerts" aria-label="SigmaUSD status">
                <div className="dash-alerts-label">SigmaUSD status</div>
                {showDivergence && dexErgUsd && divergencePct !== null && sigmaUsdState && (
                  <div className={`dash-alert ${Math.abs(divergencePct) > 10 ? 'dash-alert--danger' : 'dash-alert--warn'}`}>
                    <div className="dash-alert-body">
                      <div className="dash-alert-title">
                        {divergencePct < 0 ? 'DEX ERG below oracle' : 'DEX ERG above oracle'}
                        <span className="dash-alert-pill">
                          {divergencePct > 0 ? '+' : ''}{divergencePct.toFixed(1)}%
                        </span>
                      </div>
                      <p className="dash-alert-desc">
                        Spectrum ${dexErgUsd.toFixed(2)} vs SigmaUSD oracle ${ergUsd.toFixed(2)}.
                        {sigmaUsdState.can_mint_sigusd
                          ? ' Minting is open — possible arb.'
                          : ` Minting closed until reserves hit 400% (now ${Math.round(sigmaUsdState.reserve_ratio_pct)}%).`}
                      </p>
                    </div>
                  </div>
                )}

                {rrRecovery && sigmaUsdState && (
                  <div className="dash-alert dash-alert--info">
                    <div className="dash-alert-body">
                      <div className="dash-alert-title">
                        Reserves below mint threshold
                        <span className="dash-alert-pill">
                          RR {Math.round(sigmaUsdState.reserve_ratio_pct)}% → 400%
                        </span>
                      </div>
                      <p className="dash-alert-desc">
                        SigUSD only mints above 400% collateral. Approx.{' '}
                        {rrRecovery.sigrsvNeeded > 0
                          ? `mint ${rrRecovery.sigrsvNeeded.toLocaleString()} SigRSV`
                          : 'add ERG'}
                        {rrRecovery.sigusdToRedeem > 0
                          ? ` or redeem ${rrRecovery.sigusdToRedeem.toLocaleString()} SigUSD`
                          : ''}
                        {' '}to reopen.
                      </p>
                    </div>
                    {onNavigate && (
                      <button type="button" className="dash-alert-btn" onClick={() => onNavigate('sigmausd')}>
                        Open
                      </button>
                    )}
                  </div>
                )}
              </section>
            )}

            {onNavigate && (
              <section className="dash-protocols">
                <div className="dash-protocol-grid">
                  {PROTOCOL_GRID.map(p => (
                    <button
                      key={p.id}
                      type="button"
                      className="dash-protocol-tile"
                      onClick={() => onNavigate(p.id)}
                      style={{ '--tile-accent': p.accent } as CSSProperties}
                    >
                      {p.icon ? (
                        <img src={p.icon} alt="" className="dash-protocol-tile-icon" />
                      ) : (
                        <span className="dash-protocol-tile-fallback" aria-hidden>
                          {p.name[0]}
                        </span>
                      )}
                      <span className="dash-protocol-tile-text">
                        <span className="dash-protocol-tile-name">{p.name}</span>
                        <span className="dash-protocol-tile-desc">{p.desc}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            )}
          </div>

          <div className="dash-body">
            <section className="dash-panel">
              <div className="dash-panel-head">
                <h2 className="dash-section-title">Protocol status</h2>
              </div>
              <div className="dash-panel-scroll">
                {!sigmaUsdState && !dexyGold ? (
                  <div className="dash-feed-state">Loading protocol status…</div>
                ) : (
                  <div className="dash-status-stack">
                    {sigmaUsdState && (
                      <article className="dash-status-card">
                        <header className="dash-status-header">
                          <div className="dash-status-title">
                            <img src="/icons/sigmausd.svg" alt="" />
                            <span>SigmaUSD</span>
                          </div>
                          <span className={`dash-rr dash-rr--${rrTone(sigmaUsdState.reserve_ratio_pct)}`}>
                            RR {Math.round(sigmaUsdState.reserve_ratio_pct)}%
                          </span>
                        </header>
                        <div className="dash-ops">
                          <div className="dash-op">
                            <div className="dash-op-head">
                              <img src="/icons/sigusd.svg" alt="" />
                              <span>SigUSD</span>
                            </div>
                            <div className="dash-op-badges">
                              <span className={`dash-badge ${sigmaUsdState.can_mint_sigusd ? 'open' : 'closed'}`}>Mint</span>
                              <span className={`dash-badge ${sigmaUsdState.can_redeem_sigusd ? 'open' : 'closed'}`}>Redeem</span>
                            </div>
                            <span className="dash-op-stat mono">
                              {(sigmaUsdState.sigusd_circulating / Math.pow(10, TOKEN_DECIMALS.SigUSD)).toLocaleString()} circulating
                            </span>
                          </div>
                          <div className="dash-op">
                            <div className="dash-op-head">
                              <img src="/icons/sigrsv.svg" alt="" />
                              <span>SigRSV</span>
                            </div>
                            <div className="dash-op-badges">
                              <span className={`dash-badge ${sigmaUsdState.can_mint_sigrsv ? 'open' : 'closed'}`}>Mint</span>
                              <span className={`dash-badge ${sigmaUsdState.can_redeem_sigrsv ? 'open' : 'closed'}`}>Redeem</span>
                            </div>
                            <span className="dash-op-stat mono">
                              {sigmaUsdState.sigrsv_circulating.toLocaleString()} circulating
                            </span>
                          </div>
                        </div>
                      </article>
                    )}

                    {dexyGold && dexyUsd && (
                      <article className="dash-status-card">
                        <header className="dash-status-header">
                          <div className="dash-status-title">
                            <img src="/icons/dexygold.svg" alt="" />
                            <span>Dexy</span>
                          </div>
                        </header>
                        <div className="dash-ops">
                          <div className="dash-op">
                            <div className="dash-op-head">
                              <img src="/icons/dexygold.svg" alt="" />
                              <span>DexyGold</span>
                            </div>
                            <div className="dash-op-badges">
                              <span className={`dash-badge ${dexyGold.can_mint ? 'open' : 'closed'}`}>Mint</span>
                            </div>
                            <span className="dash-op-stat mono">
                              {dexyGold.free_mint_available.toLocaleString()} freemint
                            </span>
                            <span className="dash-op-stat mono">
                              {dexyGold.dexy_circulating.toLocaleString()} circulating
                            </span>
                          </div>
                          <div className="dash-op">
                            <div className="dash-op-head">
                              <img src="/icons/use.svg" alt="" />
                              <span>USE</span>
                            </div>
                            <div className="dash-op-badges">
                              <span className={`dash-badge ${dexyUsd.can_mint ? 'open' : 'closed'}`}>Mint</span>
                            </div>
                            <span className="dash-op-stat mono">
                              {(dexyUsd.free_mint_available / Math.pow(10, TOKEN_DECIMALS.USE)).toLocaleString()} freemint
                            </span>
                            <span className="dash-op-stat mono">
                              {(dexyUsd.dexy_circulating / Math.pow(10, TOKEN_DECIMALS.USE)).toLocaleString()} circulating
                            </span>
                          </div>
                        </div>
                      </article>
                    )}
                  </div>
                )}
              </div>
            </section>

            <section className="dash-panel">
              <div className="dash-panel-head dash-panel-head--tabs">
                <div className="dash-feed-tabs" role="tablist" aria-label="Activity feeds">
                  <button
                    type="button"
                    role="tab"
                    aria-selected={feedTab === 'activity'}
                    className={`dash-feed-tab ${feedTab === 'activity' ? 'active' : ''}`}
                    onClick={() => setFeedTab('activity')}
                  >
                    Markets
                  </button>
                  {walletBalance && (
                    <button
                      type="button"
                      role="tab"
                      aria-selected={feedTab === 'wallet'}
                      className={`dash-feed-tab ${feedTab === 'wallet' ? 'active' : ''}`}
                      onClick={() => setFeedTab('wallet')}
                    >
                      Wallet
                    </button>
                  )}
                </div>
              </div>
              <div className="dash-panel-scroll dash-feed">
                {feedTab === 'wallet' && walletBalance ? renderWalletRows() : renderActivityRows()}
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  )
}
