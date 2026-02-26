import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { TransactionModal, type SigmaUsdAction } from './TransactionModal'
import { getSigmaUsdActivity, type ProtocolInteraction } from '../api/protocolActivity'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
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

  // Initial protocol activity fetch
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

  // Initial user tx fetch
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
      <div className="sigmausd-view">
        <div className="empty-state">
          <p>Connect to a node first</p>
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="sigmausd-view">
        <div className="message error">
          SigmaUSD requires an indexed node with extraIndex enabled.
        </div>
      </div>
    )
  }

  if (loading && !state) {
    return (
      <div className="sigmausd-view">
        <div className="empty-state">
          <p>Loading protocol state...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="sigmausd-view">
        <div className="message error">{error}</div>
      </div>
    )
  }

  if (!state) {
    return (
      <div className="sigmausd-view">
        <div className="empty-state">
          <p>Unable to load protocol state</p>
        </div>
      </div>
    )
  }

  const canMintSigusd = walletAddress && state.can_mint_sigusd
  const canRedeemSigusd = walletAddress && state.can_redeem_sigusd
  const canMintSigrsv = walletAddress && state.can_mint_sigrsv
  const canRedeemSigrsv = walletAddress && state.can_redeem_sigrsv

  // Derived values
  const ergUsd = 1e9 / state.oracle_erg_per_usd_nano
  const ergReserves = state.bank_erg_nano / 1e9
  const liabilitiesErg = state.liabilities_nano / 1e9
  const equityErg = state.equity_nano / 1e9
  const sigusdSupply = state.sigusd_circulating / 100
  const sigrsvSupply = state.sigrsv_circulating
  const sigusdPrice = state.sigusd_price_nano / 1e9
  const sigrsvPrice = state.sigrsv_price_nano / 1e9

  // Format compact numbers
  const formatCompact = (n: number) => {
    if (n >= 1e9) return (n / 1e9).toFixed(2) + 'B'
    if (n >= 1e6) return (n / 1e6).toFixed(2) + 'M'
    if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K'
    return n.toFixed(2)
  }

  // Gauge calculations
  const ratio = state.reserve_ratio_pct
  const clampedRatio = Math.min(Math.max(ratio, 0), 1000)
  const percentage = Math.min((clampedRatio / 1000) * 100, 100)
  const circumference = 2 * Math.PI * 40
  const strokeDashoffset = circumference - (percentage / 100) * circumference

  const getGaugeStatus = () => {
    if (ratio < 100) return { color: '#EF4444', label: 'Critical', cls: 'critical' }
    if (ratio < 400) return { color: '#F59E0B', label: 'Below Minimum', cls: 'danger' }
    if (ratio > 800) return { color: '#3B82F6', label: 'Above Maximum', cls: 'excess' }
    return { color: '#10B981', label: 'Healthy', cls: 'healthy' }
  }
  const gaugeStatus = getGaugeStatus()

  // Wallet values
  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const sigusdBalance = walletBalance ? walletBalance.sigusd_amount / 100 : 0
  const sigrsvBalance = walletBalance ? walletBalance.sigrsv_amount : 0
  const ergValue = ergBalance * ergUsd
  const sigusdValue = sigusdBalance * sigusdPrice * ergUsd
  const sigrsvValue = sigrsvBalance * sigrsvPrice * ergUsd
  const totalValue = ergValue + sigusdValue + sigrsvValue

  return (
    <div className="sigmausd-view">
      <div className="sigmausd-content">
        {/* Protocol Header */}
        <div className="sigmausd-header">
          <div className="sigmausd-header-row">
            <div className="sigmausd-icon-stack">
              <span className="icon-sigusd">
                <img src="/icons/sigmausd.svg" alt="SigUSD" />
              </span>
              <span className="icon-sigrsv">
                <img src="/icons/sigrsv.svg" alt="SigRSV" />
              </span>
            </div>
            <div>
              <h2>SigmaUSD Protocol</h2>
              <p className="sigmausd-description">Algorithmic stablecoin with reserve-backed stability</p>
            </div>
          </div>
        </div>

        {/* Protocol Info Bar */}
        <div className="protocol-info-bar">
          <div className="info-item">
            <span className="info-label">Protocol Fee:</span>
            <span className="info-value">2%</span>
          </div>
          <div className="info-divider" />
          <div className="info-item">
            <span className="info-label">Reserve Range:</span>
            <span className="info-value">400% – 800%</span>
          </div>
          <div className="info-divider" />
          <div className="info-item">
            <span className="info-label">Bank Box:</span>
            <span className="info-value mono">{state.bank_box_id.slice(0, 8)}...{state.bank_box_id.slice(-4)}</span>
          </div>
          <div className="info-status">
            <span className="dot" />
            <span className="info-label">Live</span>
          </div>
        </div>

        {/* Reserve Ratio Section */}
        <div className="reserve-section">
          <div className="reserve-layout">
            {/* Reserve Gauge */}
            <div className="reserve-gauge-container">
              <div className="gauge-wrapper">
                <svg className="gauge-svg" viewBox="0 0 100 100">
                  <circle className="gauge-bg" cx="50" cy="50" r="40" />
                  <circle
                    className="gauge-progress"
                    cx="50" cy="50" r="40"
                    stroke={gaugeStatus.color}
                    strokeDasharray={circumference}
                    strokeDashoffset={strokeDashoffset}
                  />
                </svg>
                <div className="gauge-center">
                  <span className={`gauge-value ${gaugeStatus.cls}`}>{ratio.toFixed(0)}%</span>
                  <span className="gauge-label">Reserve Ratio</span>
                </div>
              </div>
              <div className={`gauge-status ${gaugeStatus.cls}`}>{gaugeStatus.label}</div>
              <div className="gauge-range">
                <span>Min 400%</span>
                <span>•</span>
                <span>Max 800%</span>
              </div>
            </div>

            {/* Stats Grid */}
            <div className="stats-grid">
              <div className="stat-card">
                <div className="stat-header">
                  <svg className="stat-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M12 6v6l4 2" />
                  </svg>
                  <span className="stat-label">ERG/USD</span>
                </div>
                <div className="stat-value-row">
                  <span className="stat-value">${ergUsd.toFixed(4)}</span>
                </div>
                <div className="stat-subtext">Oracle Price</div>
              </div>

              <div className="stat-card">
                <div className="stat-header">
                  <svg className="stat-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                  </svg>
                  <span className="stat-label">Reserves</span>
                </div>
                <div className="stat-value-row">
                  <span className="stat-value">{formatCompact(ergReserves)}</span>
                  <span className="stat-unit">ERG</span>
                </div>
                <div className="stat-subtext">${formatCompact(ergReserves * ergUsd)}</div>
              </div>

              <div className="stat-card">
                <div className="stat-header">
                  <svg className="stat-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
                  </svg>
                  <span className="stat-label">Liabilities</span>
                </div>
                <div className="stat-value-row">
                  <span className="stat-value">{formatCompact(liabilitiesErg)}</span>
                  <span className="stat-unit">ERG</span>
                </div>
                <div className="stat-subtext">${formatCompact(liabilitiesErg * ergUsd)}</div>
              </div>

              <div className="stat-card highlight">
                <div className="stat-header">
                  <svg className="stat-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M12 2L2 7l10 5 10-5-10-5z" />
                    <path d="M2 17l10 5 10-5M2 12l10 5 10-5" />
                  </svg>
                  <span className="stat-label">Equity</span>
                </div>
                <div className="stat-value-row">
                  <span className="stat-value">{formatCompact(equityErg)}</span>
                  <span className="stat-unit">ERG</span>
                </div>
                <div className="stat-subtext">${formatCompact(equityErg * ergUsd)}</div>
              </div>
            </div>
          </div>
        </div>

        {/* Token Cards */}
        <div className="token-cards-grid">
          {/* SigUSD Card */}
          <div className="token-card emerald">
            <div className="token-card-header">
              <div className="token-header-content">
                <div className="token-header-left">
                  <img src="/icons/sigmausd.svg" alt="SigUSD" className="token-icon" />
                  <div className="token-info">
                    <h3>SigUSD</h3>
                    <p>Algorithmic stablecoin pegged to USD</p>
                  </div>
                </div>
                <span className="token-ticker">SIGUSD</span>
              </div>
            </div>
            <div className="token-card-body">
              <div className="token-stats">
                <div className="token-stat">
                  <span className="token-stat-label">Circulating Supply</span>
                  <span className="token-stat-value">{formatCompact(sigusdSupply)}</span>
                </div>
                <div className="token-stat">
                  <span className="token-stat-label">Price (ERG)</span>
                  <span className="token-stat-value">{sigusdPrice.toFixed(4)}</span>
                </div>
                <div className="token-stat">
                  <span className="token-stat-label">Price (USD)</span>
                  <span className="token-stat-value">${(sigusdPrice * ergUsd).toFixed(4)}</span>
                </div>
              </div>

              {walletAddress && (
                <div className="wallet-balance-box">
                  <div className="wallet-balance-row">
                    <span className="wallet-balance-label">Your Balance</span>
                    <div className="wallet-balance-value">
                      <span className="wallet-balance-amount">
                        {sigusdBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
                      </span>
                      <span className="wallet-balance-ticker">SIGUSD</span>
                    </div>
                  </div>
                </div>
              )}

              <div className="token-actions">
                <button
                  className={`action-btn ${canMintSigusd ? 'primary emerald' : ''}`}
                  disabled={!canMintSigusd}
                  onClick={() => openTxModal('mint_sigusd')}
                  title={!walletAddress ? 'Connect wallet to mint' : !state.can_mint_sigusd ? 'Minting unavailable' : 'Mint SigUSD'}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M12 4v16m8-8H4" />
                  </svg>
                  Mint
                </button>
                <button
                  className={`action-btn ${canRedeemSigusd && sigusdBalance > 0 ? 'secondary' : ''}`}
                  disabled={!canRedeemSigusd || sigusdBalance <= 0}
                  onClick={() => openTxModal('redeem_sigusd')}
                  title={!walletAddress ? 'Connect wallet to redeem' : !state.can_redeem_sigusd ? 'Redemption unavailable' : sigusdBalance <= 0 ? 'No SigUSD to redeem' : 'Redeem SigUSD'}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M20 12H4" />
                  </svg>
                  Redeem
                </button>
              </div>

              <div className="status-badges">
                <span className={`status-badge ${state.can_mint_sigusd ? 'available' : 'unavailable'}`}>
                  <span className="dot" />
                  Mint {state.can_mint_sigusd ? 'Available' : 'Unavailable'}
                </span>
                <span className={`status-badge ${state.can_redeem_sigusd ? 'available' : 'unavailable'}`}>
                  <span className="dot" />
                  Redeem {state.can_redeem_sigusd ? 'Available' : 'Unavailable'}
                </span>
              </div>
            </div>
          </div>

          {/* SigRSV Card */}
          <div className="token-card blue">
            <div className="token-card-header">
              <div className="token-header-content">
                <div className="token-header-left">
                  <img src="/icons/sigrsv.svg" alt="SigRSV" className="token-icon" />
                  <div className="token-info">
                    <h3>SigRSV</h3>
                    <p>Reserve token backing SigUSD</p>
                  </div>
                </div>
                <span className="token-ticker">SIGRSV</span>
              </div>
            </div>
            <div className="token-card-body">
              <div className="token-stats">
                <div className="token-stat">
                  <span className="token-stat-label">Circulating Supply</span>
                  <span className="token-stat-value">{formatCompact(sigrsvSupply)}</span>
                </div>
                <div className="token-stat">
                  <span className="token-stat-label">Price (ERG)</span>
                  <span className="token-stat-value">{sigrsvPrice.toFixed(8)}</span>
                </div>
                <div className="token-stat">
                  <span className="token-stat-label">Price (USD)</span>
                  <span className="token-stat-value">${(sigrsvPrice * ergUsd).toFixed(6)}</span>
                </div>
              </div>

              {walletAddress && (
                <div className="wallet-balance-box">
                  <div className="wallet-balance-row">
                    <span className="wallet-balance-label">Your Balance</span>
                    <div className="wallet-balance-value">
                      <span className="wallet-balance-amount">
                        {sigrsvBalance.toLocaleString()}
                      </span>
                      <span className="wallet-balance-ticker">SIGRSV</span>
                    </div>
                  </div>
                </div>
              )}

              <div className="token-actions">
                <button
                  className={`action-btn ${canMintSigrsv ? 'primary blue' : ''}`}
                  disabled={!canMintSigrsv}
                  onClick={() => openTxModal('mint_sigrsv')}
                  title={!walletAddress ? 'Connect wallet to mint' : !state.can_mint_sigrsv ? 'Minting unavailable' : 'Mint SigRSV'}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M12 4v16m8-8H4" />
                  </svg>
                  Mint
                </button>
                <button
                  className={`action-btn ${canRedeemSigrsv && sigrsvBalance > 0 ? 'secondary' : ''}`}
                  disabled={!canRedeemSigrsv || sigrsvBalance <= 0}
                  onClick={() => openTxModal('redeem_sigrsv')}
                  title={!walletAddress ? 'Connect wallet to redeem' : !state.can_redeem_sigrsv ? 'Redemption unavailable' : sigrsvBalance <= 0 ? 'No SigRSV to redeem' : 'Redeem SigRSV'}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M20 12H4" />
                  </svg>
                  Redeem
                </button>
              </div>

              <div className="status-badges">
                <span className={`status-badge ${state.can_mint_sigrsv ? 'available' : 'unavailable'}`}>
                  <span className="dot" />
                  Mint {state.can_mint_sigrsv ? 'Available' : 'Unavailable'}
                </span>
                <span className={`status-badge ${state.can_redeem_sigrsv ? 'available' : 'unavailable'}`}>
                  <span className="dot" />
                  Redeem {state.can_redeem_sigrsv ? 'Available' : 'Unavailable'}
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* Wallet Section */}
        {walletAddress ? (
          <div className="wallet-section">
            <div className="wallet-tabs">
              <button className="wallet-tab active">Your Holdings</button>
            </div>
            <div className="wallet-tab-content">
              <div className="portfolio-total">
                <div className="portfolio-total-label">Total Portfolio Value</div>
                <div className="portfolio-total-value">${totalValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>
              </div>
              <div className="holdings-grid">
                <div className="holding-card orange">
                  <div className="holding-header">
                    <div className="holding-icon" style={{ background: 'rgba(249, 115, 22, 0.3)', width: 32, height: 32, borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '1rem', fontWeight: 700, color: '#fb923c' }}>Σ</div>
                    <span className="holding-name">ERG</span>
                  </div>
                  <div className="holding-amount">{ergBalance.toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })}</div>
                  <div className="holding-usd">${ergValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>
                </div>
                <div className="holding-card emerald">
                  <div className="holding-header">
                    <img src="/icons/sigmausd.svg" alt="SigUSD" className="holding-icon" />
                    <span className="holding-name">SigUSD</span>
                  </div>
                  <div className="holding-amount">{sigusdBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</div>
                  <div className="holding-usd">${sigusdValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>
                </div>
                <div className="holding-card blue">
                  <div className="holding-header">
                    <img src="/icons/sigrsv.svg" alt="SigRSV" className="holding-icon" />
                    <span className="holding-name">SigRSV</span>
                  </div>
                  <div className="holding-amount">{sigrsvBalance.toLocaleString()}</div>
                  <div className="holding-usd">${sigrsvValue.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="wallet-section">
            <div className="wallet-notice">
              <div className="wallet-notice-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="5" width="20" height="14" rx="2" />
                  <path d="M2 10h20" />
                </svg>
              </div>
              <h3>Wallet Not Connected</h3>
              <p>Connect your wallet using the button in the header to mint and redeem tokens</p>
            </div>
          </div>
        )}

        {/* Activity Feeds — side by side */}
        <div className="sigmausd-activity-grid">
          {/* Your SigmaUSD Activity */}
          <div className="sigmausd-activity-section">
            <h3 className="sigmausd-section-header">Your SigmaUSD Activity</h3>
            <div className="sigmausd-activity-card">
              {!walletAddress ? (
                <div className="sigmausd-activity-empty">Connect wallet to see your activity</div>
              ) : userTxsLoading ? (
                <div className="sigmausd-activity-loading">
                  <div className="spinner-small" />
                  <span>Loading...</span>
                </div>
              ) : userTxs.length === 0 ? (
                <div className="sigmausd-activity-empty">No recent SigmaUSD transactions</div>
              ) : (
                <div className="sigmausd-activity-list">
                  {userTxs.map(tx => {
                    const sigmaChanges = tx.token_changes.filter(tc => SIGMAUSD_TOKEN_ID_SET.has(tc.token_id))
                    const ergChange = tx.erg_change_nano / 1e9
                    const isReceive = tx.erg_change_nano > 0
                    return (
                      <div
                        key={tx.tx_id}
                        className="sigmausd-activity-row"
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
                          {sigmaChanges.map(tc => {
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
          <div className="sigmausd-activity-section">
            <h3 className="sigmausd-section-header">Recent Protocol Activity</h3>
            <div className="sigmausd-activity-card">
              {activityLoading ? (
                <div className="sigmausd-activity-loading">
                  <div className="spinner-small" />
                  <span>Loading activity...</span>
                </div>
              ) : activity.length === 0 ? (
                <div className="sigmausd-activity-empty">No recent SigmaUSD protocol activity</div>
              ) : (
                <div className="sigmausd-activity-list">
                  {activity.map((item, idx) => {
                    const isMint = item.operation === 'mint'
                    const ergAbs = Math.abs(item.erg_change_nano) / 1e9
                    const icon = TOKEN_ICONS[item.token]
                    return (
                      <div
                        key={`${item.tx_id}-${idx}`}
                        className="sigmausd-activity-row"
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
                              <span className={`sigmausd-token-icon-wrap ${item.token === 'SigUSD' ? 'sigusd' : 'sigrsv'}`}>
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

        {/* Transaction Modal */}
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
    </div>
  )
}
