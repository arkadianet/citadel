import { useState, useEffect, useCallback, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { discoverNodes, type NodeProbeResult } from './api/nodes'
import { WalletConnect } from './components/WalletConnect'
import { NotificationBell } from './components/NotificationBell'
import { ToastStack } from './components/Toast'
import { useNotifications } from './hooks/useNotifications'
import { Sidebar } from './components/Sidebar'
import { Dashboard } from './components/Dashboard'
import { SigmaUsdTab } from './components/SigmaUsdTab'
import { DexyTab } from './components/DexyTab'
import { LendingTab } from './components/LendingTab'
import { SwapTab } from './components/SwapTab'
import { ExplorerTab, type ExplorerRoute } from './components/ExplorerTab'
import { BurnTab } from './components/BurnTab'
import { UtxoManagementTab } from './components/UtxoManagementTab'
import { HodlCoinTab } from './components/HodlCoinTab'
import { BridgeTab } from './components/BridgeTab'
import { SigmaFiTab } from './components/SigmaFiTab'
import { TimelockTab } from './components/TimelockTab'
import { ExplorerNavProvider, type ExplorerTarget } from './contexts/ExplorerNavContext'
import './App.css'

interface NodeStatus {
  connected: boolean
  url: string
  node_name: string | null
  network: string
  chain_height: number
  indexed_height: number | null
  capability_tier: string
  index_lag: number | null
}

interface OraclePrice {
  nanoerg_per_usd: number
  erg_usd: number
  oracle_box_id: string
}

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

type View = 'home' | 'sigmausd' | 'dexy' | 'lending' | 'dex' | 'hodlcoin' | 'bridge' | 'bonds' | 'timelocks' | 'explorer' | 'burn' | 'utxo-management'

function App() {
  const [view, setView] = useState<View>('home')
  const [nodeStatus, setNodeStatus] = useState<NodeStatus | null>(null)
  const [showSettings, setShowSettings] = useState(false)
  const [nodeUrl, setNodeUrl] = useState('http://localhost:9053')
  const [apiKey, setApiKey] = useState('')
  const [connecting, setConnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [sigmaUsdState, setSigmaUsdState] = useState<SigmaUsdState | null>(null)
  const [sigmaUsdError, setSigmaUsdError] = useState<string | null>(null)
  const [loadingSigmaUsd, setLoadingSigmaUsd] = useState(false)
  const [oraclePrice, setOraclePrice] = useState<OraclePrice | null>(null)
  const [walletAddress, setWalletAddress] = useState<string | null>(null)
  const [walletBalance, setWalletBalance] = useState<WalletBalance | null>(null)
  const [showWalletConnect, setShowWalletConnect] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [explorerUrl, setExplorerUrl] = useState(() =>
    localStorage.getItem('explorerUrl') || 'https://sigmaspace.io'
  )
  const [explorerPendingRoute, setExplorerPendingRoute] = useState<ExplorerRoute | null>(null)
  const [discoveredNodes, setDiscoveredNodes] = useState<NodeProbeResult[]>([])
  const [discovering, setDiscovering] = useState(false)
  const [rosenDisclaimed, setRosenDisclaimed] = useState(false)
  const [showRosenWarning, setShowRosenWarning] = useState(false)
  const { notifications, unreadCount, pendingCount, markAllRead } = useNotifications()

  const clearPendingRoute = useCallback(() => setExplorerPendingRoute(null), [])

  const explorerNavValue = useMemo(() => ({
    navigateToExplorer: (target: ExplorerTarget) => {
      setExplorerPendingRoute(target)
      setView('explorer')
      setSidebarCollapsed(true)
    },
  }), [])

  const fetchStatus = useCallback(async () => {
    try {
      const status = await invoke<NodeStatus>('get_node_status')
      setNodeStatus(status)
    } catch (e) {
      console.error('Failed to fetch status:', e)
    }
  }, [])

  const fetchWalletStatus = useCallback(async () => {
    try {
      const status = await invoke<{ connected: boolean; address: string | null }>('get_wallet_status')
      setWalletAddress(status.address)
    } catch (e) {
      console.error('Failed to fetch wallet status:', e)
    }
  }, [])

  const fetchOraclePrice = useCallback(async () => {
    if (!nodeStatus?.connected || nodeStatus?.capability_tier === 'Basic') {
      return
    }
    try {
      const price = await invoke<OraclePrice>('get_oracle_price')
      setOraclePrice(price)
    } catch (e) {
      console.error('Failed to fetch oracle price:', e)
      setOraclePrice(null)
    }
  }, [nodeStatus?.connected, nodeStatus?.capability_tier])

  const fetchSigmaUsdState = useCallback(async () => {
    if (!nodeStatus?.connected || nodeStatus?.capability_tier === 'Basic') {
      return
    }
    setLoadingSigmaUsd(true)
    try {
      const state = await invoke<SigmaUsdState>('get_sigmausd_state')
      setSigmaUsdState(state)
      setSigmaUsdError(null)
    } catch (e) {
      console.error('Failed to fetch SigmaUSD state:', e)
      setSigmaUsdError(String(e))
      setSigmaUsdState(null)
    } finally {
      setLoadingSigmaUsd(false)
    }
  }, [nodeStatus?.connected, nodeStatus?.capability_tier])

  const fetchWalletBalance = useCallback(async () => {
    if (!walletAddress || !nodeStatus?.connected || nodeStatus?.capability_tier === 'Basic') {
      setWalletBalance(null)
      return
    }
    try {
      const balance = await invoke<WalletBalance>('get_wallet_balance')
      setWalletBalance(balance)
    } catch (e) {
      console.error('Failed to fetch wallet balance:', e)
      setWalletBalance(null)
    }
  }, [walletAddress, nodeStatus?.connected, nodeStatus?.capability_tier])

  useEffect(() => {
    fetchStatus()
    const interval = setInterval(fetchStatus, 10000)
    return () => clearInterval(interval)
  }, [fetchStatus])

  useEffect(() => {
    fetchWalletStatus()
  }, [fetchWalletStatus])

  useEffect(() => {
    if (nodeStatus?.connected && nodeStatus?.capability_tier !== 'Basic') {
      fetchOraclePrice()
      const interval = setInterval(fetchOraclePrice, 30000)
      return () => clearInterval(interval)
    } else {
      setOraclePrice(null)
    }
  }, [nodeStatus?.connected, nodeStatus?.capability_tier, fetchOraclePrice])

  useEffect(() => {
    if (nodeStatus?.connected && nodeStatus?.capability_tier !== 'Basic') {
      fetchSigmaUsdState()
      const interval = setInterval(fetchSigmaUsdState, 30000)
      return () => clearInterval(interval)
    } else {
      setSigmaUsdState(null)
      setSigmaUsdError(null)
    }
  }, [nodeStatus?.connected, nodeStatus?.capability_tier, fetchSigmaUsdState])

  useEffect(() => {
    if (walletAddress && nodeStatus?.connected && nodeStatus?.capability_tier !== 'Basic') {
      fetchWalletBalance()
      const interval = setInterval(fetchWalletBalance, 30000)
      return () => clearInterval(interval)
    } else {
      setWalletBalance(null)
    }
  }, [walletAddress, nodeStatus?.connected, nodeStatus?.capability_tier, fetchWalletBalance])

  const handleConnect = async (overrideUrl?: string) => {
    const connectUrl = overrideUrl ?? nodeUrl
    setConnecting(true)
    setError(null)
    try {
      const status = await invoke<NodeStatus>('configure_node', {
        request: { url: connectUrl, api_key: apiKey }
      })
      setNodeStatus(status)
      if (!status.connected) {
        setError('Could not connect to node')
      } else {
        setShowSettings(false)
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setConnecting(false)
    }
  }

  const handleDiscoverNodes = async () => {
    setDiscovering(true)
    try {
      const nodes = await discoverNodes()
      setDiscoveredNodes(nodes)
    } catch (e) {
      console.error('Failed to discover nodes:', e)
    } finally {
      setDiscovering(false)
    }
  }

  const handleSelectNode = (url: string) => {
    setNodeUrl(url)
    handleConnect(url)
  }

  const shortenUrl = (url: string) => {
    return url.replace(/^https?:\/\//, '').replace(/:9053$/, '')
  }

  const isConnected = nodeStatus?.connected ?? false

  return (
    <ExplorerNavProvider value={explorerNavValue}>
    <div className="app">
      {/* Header */}
      <header className="header">
        <div className="header-left">
          {view !== 'home' && (
            <button className="back-btn" onClick={() => setView('home')}>
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M19 12H5M12 19l-7-7 7-7"/>
              </svg>
            </button>
          )}
          <div className="logo">
            <img src="/citadel-logo-header.svg" alt="Citadel" className="logo-lockup" />
          </div>
        </div>

        <div className="header-center">
          {isConnected && nodeStatus && (
            <div className="node-status-bar">
              {oraclePrice && (
                <>
                  <span className="status-price">${oraclePrice.erg_usd.toFixed(2)}</span>
                  <span className="status-divider">|</span>
                </>
              )}
              {nodeStatus.node_name && (
                <>
                  <span className="status-node-name">{nodeStatus.node_name}</span>
                  <span className="status-divider">|</span>
                </>
              )}
              <span className="status-network">{nodeStatus.network}</span>
              <span className="status-divider">|</span>
              <span className="status-height">{nodeStatus.chain_height.toLocaleString()}</span>
              <span className="status-divider">|</span>
              <span className={`status-tier ${nodeStatus.capability_tier.toLowerCase()}`}>
                {nodeStatus.capability_tier}
              </span>
            </div>
          )}
        </div>

        <div className="header-right">
          {walletAddress ? (
            <div className="wallet-info">
              {walletBalance && (
                <span className="wallet-balance">{walletBalance.erg_formatted} ERG</span>
              )}
              <button
                className="wallet-indicator"
                onClick={() => {
                  invoke('disconnect_wallet').then(() => {
                    setWalletAddress(null)
                    setWalletBalance(null)
                  })
                }}
                title="Click to disconnect"
              >
                {walletAddress.slice(0, 6)}...{walletAddress.slice(-4)}
              </button>
            </div>
          ) : (
            <div className="header-wallet-connect">
              <button
                className="connect-wallet-btn"
                onClick={() => setShowWalletConnect(true)}
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="5" width="20" height="14" rx="2" />
                  <path d="M2 10h20" />
                </svg>
                Connect Wallet
              </button>
            </div>
          )}
          <NotificationBell
            notifications={notifications}
            unreadCount={unreadCount}
            pendingCount={pendingCount}
            onMarkAllRead={markAllRead}
          />
          <div className={`connection-indicator ${isConnected ? 'connected' : ''}`} />
          <button className="settings-btn" onClick={() => setShowSettings(true)}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z"/>
            </svg>
          </button>
        </div>
      </header>

      {/* Wallet Connect Modal */}
      {showWalletConnect && (
        <div className="modal-overlay" onClick={() => setShowWalletConnect(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Connect Wallet</h2>
              <button className="close-btn" onClick={() => setShowWalletConnect(false)}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12"/>
                </svg>
              </button>
            </div>
            <div className="modal-content">
              <WalletConnect
                onConnected={(address) => {
                  setWalletAddress(address)
                  setShowWalletConnect(false)
                }}
                onCancel={() => setShowWalletConnect(false)}
                onClose={() => setShowWalletConnect(false)}
              />
            </div>
          </div>
        </div>
      )}

      {/* App Body: Sidebar + Main */}
      <div className="app-body">
        <Sidebar
          view={view}
          onNavigate={(v) => {
            if (v === 'bridge' && !rosenDisclaimed) {
              setShowRosenWarning(true)
              return
            }
            setView(v)
            if (v !== 'home') setSidebarCollapsed(true)
          }}
          isConnected={isConnected}
          capabilityTier={nodeStatus?.capability_tier}
          collapsed={view !== 'home' || sidebarCollapsed}
          onToggleCollapse={() => setSidebarCollapsed(c => !c)}
        />

        <main className="main">
          {view === 'home' && (
            <Dashboard
              isConnected={isConnected}
              ergUsd={oraclePrice?.erg_usd ?? 0}
              walletBalance={walletBalance}
              sigmaUsdState={sigmaUsdState}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'sigmausd' && (
            <SigmaUsdTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              state={sigmaUsdState}
              error={sigmaUsdError}
              loading={loadingSigmaUsd}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'dexy' && (
            <DexyTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              ergUsdPrice={oraclePrice?.erg_usd}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'lending' && (
            <LendingTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              onWalletConnected={setWalletAddress}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'dex' && (
            <SwapTab
              isConnected={nodeStatus?.connected ?? false}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
              ergUsdPrice={oraclePrice?.erg_usd ?? 0}
              canMintSigusd={sigmaUsdState?.can_mint_sigusd ?? false}
              reserveRatioPct={sigmaUsdState?.reserve_ratio_pct ?? 0}
            />
          )}

          {view === 'hodlcoin' && (
            <HodlCoinTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'bridge' && (
            <BridgeTab
              isConnected={isConnected}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'bonds' && (
            <SigmaFiTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'timelocks' && (
            <TimelockTab
              isConnected={isConnected}
              capabilityTier={nodeStatus?.capability_tier}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'explorer' && (
            <ExplorerTab
              isConnected={isConnected}
              explorerUrl={explorerUrl}
              pendingRoute={explorerPendingRoute}
              onPendingRouteConsumed={clearPendingRoute}
            />
          )}

          {view === 'burn' && (
            <BurnTab
              isConnected={isConnected}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}

          {view === 'utxo-management' && (
            <UtxoManagementTab
              isConnected={isConnected}
              walletAddress={walletAddress}
              walletBalance={walletBalance}
              explorerUrl={explorerUrl}
            />
          )}
        </main>
      </div>

      {/* Settings Modal */}
      {showSettings && (
        <div className="modal-overlay" onClick={() => setShowSettings(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Settings</h2>
              <button className="close-btn" onClick={() => setShowSettings(false)}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12"/>
                </svg>
              </button>
            </div>

            <div className="modal-content">
              <div className="settings-section">
                <h3>Node Connection</h3>

                <div className="form-group">
                  <label className="form-label">Node URL</label>
                  <input
                    type="text"
                    className="input"
                    value={nodeUrl}
                    onChange={(e) => setNodeUrl(e.target.value)}
                    placeholder="http://localhost:9053"
                  />
                </div>

                <div className="form-group">
                  <label className="form-label">API Key (optional)</label>
                  <input
                    type="password"
                    className="input"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="For authenticated endpoints"
                  />
                </div>

                <button
                  className="btn btn-primary"
                  onClick={() => handleConnect()}
                  disabled={connecting || !nodeUrl}
                >
                  {connecting ? 'Connecting...' : isConnected ? 'Reconnect' : 'Connect'}
                </button>

                {error && <div className="message error">{error}</div>}

                {isConnected && nodeStatus && (
                  <div className="connection-info">
                    <div className="info-row">
                      <span>Status</span>
                      <span className="text-success">Connected</span>
                    </div>
                    <div className="info-row">
                      <span>URL</span>
                      <span>{nodeStatus.url}</span>
                    </div>
                    <div className="info-row">
                      <span>Capability</span>
                      <span>{nodeStatus.capability_tier}</span>
                    </div>
                  </div>
                )}
              </div>

              <div className="settings-section">
                <div className="node-list-header">
                  <h3>Public Nodes</h3>
                  <button
                    className="btn-discover"
                    onClick={handleDiscoverNodes}
                    disabled={discovering}
                  >
                    {discovering ? (
                      <>
                        <span className="spinner-tiny" />
                        Probing...
                      </>
                    ) : (
                      'Discover Peers'
                    )}
                  </button>
                </div>

                <div className="node-list">
                  {discoveredNodes.length === 0 && !discovering && (
                    <div className="node-list-empty">
                      Click "Discover Peers" to find available nodes
                    </div>
                  )}
                  {discovering && discoveredNodes.length === 0 && (
                    <div className="node-list-empty">
                      <span className="spinner-small" />
                      Probing nodes...
                    </div>
                  )}
                  {discoveredNodes.map((node) => (
                    <button
                      key={node.url}
                      className="node-list-row"
                      onClick={() => handleSelectNode(node.url)}
                      disabled={connecting}
                    >
                      <div className="node-row-info">
                        <span className="node-row-url">{shortenUrl(node.url)}</span>
                        {node.name && <span className="node-row-name">{node.name}</span>}
                      </div>
                      <div className="node-row-meta">
                        <span className={`tier-badge tier-${node.capability_tier.toLowerCase()}`}>
                          {node.capability_tier}
                        </span>
                        <span className="node-row-latency">{node.latency_ms}ms</span>
                        <svg className="node-row-arrow" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M5 12h14M12 5l7 7-7 7" />
                        </svg>
                      </div>
                    </button>
                  ))}
                </div>
              </div>

              <div className="settings-section">
                <h3>Explorer</h3>
                <div className="form-group">
                  <label className="form-label">Explorer URL</label>
                  <input
                    type="text"
                    className="input"
                    value={explorerUrl}
                    onChange={(e) => {
                      setExplorerUrl(e.target.value)
                      localStorage.setItem('explorerUrl', e.target.value)
                    }}
                    placeholder="https://sigmaspace.io"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
      <ToastStack notifications={notifications} />

      {showRosenWarning && (
        <div className="modal-overlay" onClick={() => setShowRosenWarning(false)}>
          <div className="modal-card" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-header">
              <h2>Rosen Bridge Warning</h2>
              <button className="modal-close" onClick={() => setShowRosenWarning(false)}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div style={{ padding: '16px 24px', lineHeight: 1.6 }}>
              <p style={{ color: 'var(--warning-color, #f59e0b)', fontWeight: 600, marginBottom: 12 }}>
                This feature is untested and likely will not work.
              </p>
              <p style={{ color: 'var(--text-secondary)', marginBottom: 16 }}>
                Only Ergo to other chains is theoretically supported in this app.
                Bridging from other chains to Ergo is not supported.
                Use at your own risk.
              </p>
              <div style={{ display: 'flex', gap: 12, justifyContent: 'flex-end' }}>
                <button className="btn btn-secondary" onClick={() => setShowRosenWarning(false)}>
                  Cancel
                </button>
                <button className="btn btn-primary" onClick={() => {
                  setRosenDisclaimed(true)
                  setShowRosenWarning(false)
                  setView('bridge')
                  setSidebarCollapsed(true)
                }}>
                  I understand, continue
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
    </ExplorerNavProvider>
  )
}

export default App
