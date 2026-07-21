import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { TransactionModal, type SigmaUsdAction } from './TransactionModal'
import { getSigmaUsdActivity, type ProtocolInteraction } from '../api/protocolActivity'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import { EmptyState } from './ui'
import './SigmaUsdTab.css'

interface SigmaUsdState {
  bank_erg_nano: number
  sigusd_circulating: number
  sigrsv_circulating: number
  bank_box_id: string
  oracle_erg_per_usd_nano: number
  oracle_box_id: string
  reserve_ratio_pct: number
  sigusd_price_nano: number
  sigrsv_price_nano: number
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

interface SigmaUsdTabProps {
  isConnected: boolean
  capabilityTier?: string
  state: SigmaUsdState | null
  error: string | null
  loading: boolean
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

const SIGMAUSD_TOKEN_IDS = {
  sigusd: '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04',
  sigrsv: '003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0',
}
const SIGMAUSD_TOKEN_ID_SET = new Set(Object.values(SIGMAUSD_TOKEN_IDS))

const TOKEN_ICONS: Record<string, string> = {
  SigUSD: '/icons/sigmausd.svg',
  SigRSV: '/icons/sigrsv.svg',
}

const TOKEN_DECIMALS: Record<string, number> = {
  SigUSD: 2,
  SigRSV: 0,
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

function formatCompact(n: number): string {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + 'B'
  if (n >= 1e6) return (n / 1e6).toFixed(2) + 'M'
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K'
  return n.toFixed(2)
}

export function SigmaUsdTab({
  isConnected,
  capabilityTier,
  state,
  error,
  loading,
  walletAddress,
  walletBalance,
  explorerUrl,
}: SigmaUsdTabProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [txModalOpen, setTxModalOpen] = useState(false)
  const [txAction, setTxAction] = useState<SigmaUsdAction>('mint_sigusd')
  const [activity, setActivity] = useState<ProtocolInteraction[]>([])
  const [activityLoading, setActivityLoading] = useState(false)
  const [userTxs, setUserTxs] = useState<RecentTx[]>([])
  const [userTxsLoading, setUserTxsLoading] = useState(false)
  const [feedTab, setFeedTab] = useState<'yours' | 'protocol'>('protocol')

  const openTxModal = (action: SigmaUsdAction) => {
    setTxAction(action)
    setTxModalOpen(true)
  }

  const fetchSigmaUsdActivity = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setActivityLoading(true)
    try {
      const data = await getSigmaUsdActivity(10)
      setActivity(data)
    } catch (e) {
      console.error('Failed to fetch SigmaUSD activity:', e)
      setActivity([])
    } finally {
      setActivityLoading(false)
    }
  }, [isConnected, capabilityTier])

  const fetchUserSigmaUsdTxs = useCallback(async () => {
    if (!isConnected || !walletBalance) {
      setUserTxs([])
      return
    }
    setUserTxsLoading(true)
    try {
      const res = await invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 20 })
      const sigmaTxs = res.transactions.filter(tx =>
        tx.token_changes.some(tc => SIGMAUSD_TOKEN_ID_SET.has(tc.token_id))
      )
      setUserTxs(sigmaTxs.slice(0, 10))
    } catch (e) {
      console.error('Failed to fetch user SigmaUSD transactions:', e)
      setUserTxs([])
    } finally {
      setUserTxsLoading(false)
    }
  }, [isConnected, walletBalance])

  useEffect(() => {
    let cancelled = false
    if (!isConnected || capabilityTier === 'Basic') {
      setActivity([])
      return
    }
    setActivityLoading(true)
    getSigmaUsdActivity(10)
      .then(data => { if (!cancelled) setActivity(data) })
      .catch(e => {
        console.error('Failed to fetch SigmaUSD activity:', e)
        if (!cancelled) setActivity([])
      })
      .finally(() => { if (!cancelled) setActivityLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, capabilityTier])

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
        const sigmaTxs = res.transactions.filter(tx =>
          tx.token_changes.some(tc => SIGMAUSD_TOKEN_ID_SET.has(tc.token_id))
        )
        setUserTxs(sigmaTxs.slice(0, 10))
      })
      .catch(e => {
        console.error('Failed to fetch user SigmaUSD transactions:', e)
        if (!cancelled) setUserTxs([])
      })
      .finally(() => { if (!cancelled) setUserTxsLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, walletBalance])

  if (!isConnected) {
    return (
      <div className="su-page">
        <div className="su-empty-wrap">
          <EmptyState title="Node not connected" description="Connect to a node first." />
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="su-page">
        <div className="su-empty-wrap">
          <EmptyState
            title="Indexed node required"
            description="SigmaUSD needs an indexed node with extraIndex enabled."
          />
        </div>
      </div>
    )
  }

  if (loading && !state) {
    return (
      <div className="su-page">
        <div className="su-empty-wrap">
          <div className="su-feed-state">
            <div className="spinner-small" />
            <span>Loading protocol state…</span>
          </div>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="su-page">
        <div className="su-empty-wrap">
          <EmptyState title="Error" description={error} />
        </div>
      </div>
    )
  }

  if (!state) {
    return (
      <div className="su-page">
        <div className="su-empty-wrap">
          <EmptyState title="Unable to load" description="Unable to load protocol state." />
        </div>
      </div>
    )
  }

  const canMintSigusd = walletAddress && state.can_mint_sigusd
  const canRedeemSigusd = walletAddress && state.can_redeem_sigusd
  const canMintSigrsv = walletAddress && state.can_mint_sigrsv
  const canRedeemSigrsv = walletAddress && state.can_redeem_sigrsv

  const ergUsd = 1e9 / state.oracle_erg_per_usd_nano
  const ergReserves = state.bank_erg_nano / 1e9
  const liabilitiesErg = state.liabilities_nano / 1e9
  const equityErg = state.equity_nano / 1e9
  const sigusdSupply = state.sigusd_circulating / 100
  const sigrsvSupply = state.sigrsv_circulating
  const sigusdPrice = state.sigusd_price_nano / 1e9
  const sigrsvPrice = state.sigrsv_price_nano / 1e9

  const ratio = state.reserve_ratio_pct
  const clampedRatio = Math.min(Math.max(ratio, 0), 1000)
  const percentage = Math.min((clampedRatio / 1000) * 100, 100)
  const circumference = 2 * Math.PI * 40
  const strokeDashoffset = circumference - (percentage / 100) * circumference

  const getGaugeStatus = () => {
    if (ratio < 100) return { color: '#f87171', label: 'Critical', cls: 'critical' }
    if (ratio < 400) return { color: '#fbbf24', label: 'Below minimum', cls: 'danger' }
    if (ratio > 800) return { color: '#60a5fa', label: 'Above maximum', cls: 'excess' }
    return { color: '#34d399', label: 'Healthy', cls: 'healthy' }
  }
  const gaugeStatus = getGaugeStatus()

  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const sigusdBalance = walletBalance ? walletBalance.sigusd_amount / 100 : 0
  const sigrsvBalance = walletBalance ? walletBalance.sigrsv_amount : 0
  const ergValue = ergBalance * ergUsd
  const sigusdValue = sigusdBalance * sigusdPrice * ergUsd
  const sigrsvValue = sigrsvBalance * sigrsvPrice * ergUsd
  const totalValue = ergValue + sigusdValue + sigrsvValue

  return (
    <div className="su-page">
      <header className="su-header">
        <div className="su-header-left">
          <div className="su-icon-stack" aria-hidden>
            <img src="/icons/sigmausd.svg" alt="" className="su-icon-primary" />
            <img src="/icons/sigrsv.svg" alt="" className="su-icon-secondary" />
          </div>
          <div>
            <h1 className="su-title">SigmaUSD</h1>
            <p className="su-subtitle">AgeUSD stablecoin · 2% fee · RR 400–800%</p>
          </div>
        </div>
        <div className="su-header-meta">
          <span className={`su-rr-chip su-rr-chip--${gaugeStatus.cls}`}>
            RR {ratio.toFixed(0)}% · {gaugeStatus.label}
          </span>
          <span className="su-meta-chip mono">ERG ${ergUsd.toFixed(4)}</span>
        </div>
      </header>

      <div className="su-top">
        <section className="su-health">
          <div className="su-gauge">
            <div className="su-gauge-wrap">
              <svg className="su-gauge-svg" viewBox="0 0 100 100">
                <circle className="su-gauge-bg" cx="50" cy="50" r="40" />
                <circle
                  className="su-gauge-progress"
                  cx="50" cy="50" r="40"
                  stroke={gaugeStatus.color}
                  strokeDasharray={circumference}
                  strokeDashoffset={strokeDashoffset}
                />
              </svg>
              <div className="su-gauge-center">
                <span className={`su-gauge-value ${gaugeStatus.cls}`}>{ratio.toFixed(0)}%</span>
                <span className="su-gauge-label">Reserve</span>
              </div>
            </div>
          </div>

          <div className="su-metrics">
            <div className="su-metric">
              <span className="su-metric-label">Reserves</span>
              <span className="su-metric-value mono">{formatCompact(ergReserves)} ERG</span>
              <span className="su-metric-sub">${formatCompact(ergReserves * ergUsd)}</span>
            </div>
            <div className="su-metric">
              <span className="su-metric-label">Liabilities</span>
              <span className="su-metric-value mono">{formatCompact(liabilitiesErg)} ERG</span>
              <span className="su-metric-sub">${formatCompact(liabilitiesErg * ergUsd)}</span>
            </div>
            <div className="su-metric su-metric--accent">
              <span className="su-metric-label">Equity</span>
              <span className="su-metric-value mono">{formatCompact(equityErg)} ERG</span>
              <span className="su-metric-sub">${formatCompact(equityErg * ergUsd)}</span>
            </div>
            <div className="su-metric">
              <span className="su-metric-label">SigUSD supply</span>
              <span className="su-metric-value mono">{formatCompact(sigusdSupply)}</span>
              <span className="su-metric-sub">{sigusdPrice.toFixed(4)} ERG</span>
            </div>
            <div className="su-metric">
              <span className="su-metric-label">SigRSV supply</span>
              <span className="su-metric-value mono">{formatCompact(sigrsvSupply)}</span>
              <span className="su-metric-sub">{sigrsvPrice.toFixed(6)} ERG</span>
            </div>
          </div>
        </section>

        <section className="su-ops">
          <div className="su-actions">
            <button
              type="button"
              className="su-action"
              disabled={!canMintSigusd}
              onClick={() => openTxModal('mint_sigusd')}
              title={!walletAddress ? 'Connect wallet to mint' : !state.can_mint_sigusd ? 'Minting unavailable' : 'Mint SigUSD'}
            >
              <img src="/icons/sigusd.svg" alt="" />
              <span className="su-action-label">Mint SigUSD</span>
              <span className="su-action-sub">
                {state.can_mint_sigusd ? `${sigusdPrice.toFixed(4)} ERG` : 'Closed'}
              </span>
              <span className={`su-badge ${state.can_mint_sigusd ? 'open' : 'closed'}`}>
                {state.can_mint_sigusd ? 'Open' : 'Closed'}
              </span>
            </button>
            <button
              type="button"
              className="su-action"
              disabled={!canRedeemSigusd || sigusdBalance <= 0}
              onClick={() => openTxModal('redeem_sigusd')}
              title={!walletAddress ? 'Connect wallet to redeem' : !state.can_redeem_sigusd ? 'Redemption unavailable' : sigusdBalance <= 0 ? 'No SigUSD to redeem' : 'Redeem SigUSD'}
            >
              <img src="/icons/sigusd.svg" alt="" />
              <span className="su-action-label">Redeem SigUSD</span>
              <span className="su-action-sub">
                {state.can_redeem_sigusd
                  ? (sigusdBalance > 0 ? `${sigusdBalance.toFixed(2)} available` : 'No balance')
                  : 'Closed'}
              </span>
              <span className={`su-badge ${state.can_redeem_sigusd ? 'open' : 'closed'}`}>
                {state.can_redeem_sigusd ? 'Open' : 'Closed'}
              </span>
            </button>
            <button
              type="button"
              className="su-action"
              disabled={!canMintSigrsv}
              onClick={() => openTxModal('mint_sigrsv')}
              title={!walletAddress ? 'Connect wallet to mint' : !state.can_mint_sigrsv ? 'Minting unavailable' : 'Mint SigRSV'}
            >
              <img src="/icons/sigrsv.svg" alt="" />
              <span className="su-action-label">Mint SigRSV</span>
              <span className="su-action-sub">
                {state.can_mint_sigrsv ? `${sigrsvPrice.toFixed(6)} ERG` : 'Closed'}
              </span>
              <span className={`su-badge ${state.can_mint_sigrsv ? 'open' : 'closed'}`}>
                {state.can_mint_sigrsv ? 'Open' : 'Closed'}
              </span>
            </button>
            <button
              type="button"
              className="su-action"
              disabled={!canRedeemSigrsv || sigrsvBalance <= 0}
              onClick={() => openTxModal('redeem_sigrsv')}
              title={!walletAddress ? 'Connect wallet to redeem' : !state.can_redeem_sigrsv ? 'Redemption unavailable' : sigrsvBalance <= 0 ? 'No SigRSV to redeem' : 'Redeem SigRSV'}
            >
              <img src="/icons/sigrsv.svg" alt="" />
              <span className="su-action-label">Redeem SigRSV</span>
              <span className="su-action-sub">
                {state.can_redeem_sigrsv
                  ? (sigrsvBalance > 0 ? `${sigrsvBalance.toLocaleString()} available` : 'No balance')
                  : 'Closed'}
              </span>
              <span className={`su-badge ${state.can_redeem_sigrsv ? 'open' : 'closed'}`}>
                {state.can_redeem_sigrsv ? 'Open' : 'Closed'}
              </span>
            </button>
          </div>

          {walletAddress ? (
            <div className="su-holdings">
              <div className="su-holdings-total">
                <span className="su-holdings-label">Your holdings</span>
                <span className="su-holdings-value mono">
                  ${totalValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                </span>
              </div>
              <div className="su-holding-chips">
                <div className="su-holding">
                  <span className="su-holding-icon su-holding-icon--erg">Σ</span>
                  <div>
                    <span className="su-holding-name">ERG</span>
                    <span className="su-holding-amt mono">
                      {ergBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })}
                    </span>
                  </div>
                </div>
                <div className="su-holding">
                  <img src="/icons/sigusd.svg" alt="" className="su-holding-icon" />
                  <div>
                    <span className="su-holding-name">SigUSD</span>
                    <span className="su-holding-amt mono">
                      {sigusdBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
                    </span>
                  </div>
                </div>
                <div className="su-holding">
                  <img src="/icons/sigrsv.svg" alt="" className="su-holding-icon" />
                  <div>
                    <span className="su-holding-name">SigRSV</span>
                    <span className="su-holding-amt mono">{sigrsvBalance.toLocaleString()}</span>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="su-wallet-prompt">
              Connect a wallet in the header to mint and redeem.
            </div>
          )}
        </section>
      </div>

      <section className="su-feed-panel">
        <div className="su-feed-head">
          <div className="su-feed-tabs" role="tablist" aria-label="Activity">
            <button
              type="button"
              role="tab"
              aria-selected={feedTab === 'protocol'}
              className={`su-feed-tab ${feedTab === 'protocol' ? 'active' : ''}`}
              onClick={() => setFeedTab('protocol')}
            >
              Protocol
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={feedTab === 'yours'}
              className={`su-feed-tab ${feedTab === 'yours' ? 'active' : ''}`}
              onClick={() => setFeedTab('yours')}
            >
              Yours
            </button>
          </div>
          <span className="su-feed-hint mono">
            Bank {state.bank_box_id.slice(0, 8)}…{state.bank_box_id.slice(-4)}
          </span>
        </div>

        <div className="su-feed-scroll">
          {feedTab === 'protocol' ? (
            activityLoading ? (
              <div className="su-feed-state">
                <div className="spinner-small" />
                <span>Loading activity…</span>
              </div>
            ) : activity.length === 0 ? (
              <div className="su-feed-state">No recent protocol activity</div>
            ) : (
              <div className="su-feed-list">
                {activity.map((item, idx) => {
                  const isMint = item.operation === 'mint'
                  const ergAbs = Math.abs(item.erg_change_nano) / 1e9
                  const icon = TOKEN_ICONS[item.token]
                  return (
                    <button
                      key={`${item.tx_id}-${idx}`}
                      type="button"
                      className="su-feed-row"
                      onClick={() => navigateToExplorer({ page: 'transaction', id: item.tx_id })}
                    >
                      <div className={`su-feed-icon ${isMint ? 'mint' : 'redeem'}`} aria-hidden>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                          {isMint
                            ? <path d="M12 19V5M5 12l7-7 7 7" />
                            : <path d="M12 5v14M5 12l7 7 7-7" />
                          }
                        </svg>
                      </div>
                      <div className="su-feed-info">
                        <div className="su-feed-label">
                          {icon && <img src={icon} alt="" />}
                          <span className="su-feed-op">{isMint ? 'Mint' : 'Redeem'}</span>
                          <span className="su-feed-token">{item.token}</span>
                        </div>
                        <span className="su-feed-protocol">{item.protocol}</span>
                      </div>
                      <div className="su-feed-amounts">
                        {item.token_amount_change > 0 && (() => {
                          const decimals = TOKEN_DECIMALS[item.token] ?? 0
                          const amt = decimals > 0
                            ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                            : item.token_amount_change.toLocaleString()
                          return (
                            <span className={`amt ${isMint ? 'positive' : 'negative'}`}>
                              {isMint ? '+' : '-'}{amt} {item.token}
                            </span>
                          )
                        })()}
                        {ergAbs > 0 && (
                          <span className="amt muted">
                            {ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })} ERG
                          </span>
                        )}
                      </div>
                      <span className="su-feed-time">
                        {item.timestamp > 0 ? formatTimeAgo(item.timestamp) : `#${item.height}`}
                      </span>
                    </button>
                  )
                })}
              </div>
            )
          ) : !walletAddress ? (
            <div className="su-feed-state">Connect wallet to see your activity</div>
          ) : userTxsLoading ? (
            <div className="su-feed-state">
              <div className="spinner-small" />
              <span>Loading…</span>
            </div>
          ) : userTxs.length === 0 ? (
            <div className="su-feed-state">No recent SigmaUSD transactions</div>
          ) : (
            <div className="su-feed-list">
              {userTxs.map(tx => {
                const sigmaChanges = tx.token_changes.filter(tc => SIGMAUSD_TOKEN_ID_SET.has(tc.token_id))
                const ergChange = tx.erg_change_nano / 1e9
                const isReceive = tx.erg_change_nano > 0
                return (
                  <button
                    key={tx.tx_id}
                    type="button"
                    className="su-feed-row"
                    onClick={() => navigateToExplorer({ page: 'transaction', id: tx.tx_id })}
                  >
                    <div className={`su-feed-icon ${isReceive ? 'mint' : 'redeem'}`} aria-hidden>
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                        {isReceive
                          ? <path d="M12 5v14M5 12l7 7 7-7" />
                          : <path d="M12 19V5M5 12l7-7 7 7" />
                        }
                      </svg>
                    </div>
                    <div className="su-feed-info">
                      <span className="su-feed-op mono">{tx.tx_id.slice(0, 8)}…{tx.tx_id.slice(-6)}</span>
                    </div>
                    <div className="su-feed-amounts">
                      {sigmaChanges.map(tc => {
                        const amt = tc.amount / Math.pow(10, tc.decimals)
                        const isPos = tc.amount > 0
                        return (
                          <span key={tc.token_id} className={`amt ${isPos ? 'positive' : 'negative'}`}>
                            {isPos ? '+' : ''}{amt.toLocaleString(undefined, { maximumFractionDigits: tc.decimals })} {tc.name ?? tc.token_id.slice(0, 6)}
                          </span>
                        )
                      })}
                      <span className="amt muted">
                        {isReceive ? '+' : ''}{ergChange.toLocaleString(undefined, { maximumFractionDigits: 4 })} ERG
                      </span>
                    </div>
                    <span className="su-feed-time">
                      {tx.timestamp > 0 ? formatTimeAgo(tx.timestamp) : `#${tx.inclusion_height}`}
                    </span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      </section>

      {txModalOpen && walletAddress && walletBalance && (
        <TransactionModal
          isOpen={txModalOpen}
          onClose={() => setTxModalOpen(false)}
          action={txAction}
          walletAddress={walletAddress}
          ergBalance={walletBalance.erg_nano}
          tokenBalance={
            txAction === 'redeem_sigusd' ? walletBalance.sigusd_amount :
            txAction === 'redeem_sigrsv' ? walletBalance.sigrsv_amount :
            undefined
          }
          explorerUrl={explorerUrl}
          onSuccess={(txId) => {
            console.log('Transaction successful:', txId)
            fetchSigmaUsdActivity()
            fetchUserSigmaUsdTxs()
          }}
          state={state}
        />
      )}
    </div>
  )
}
