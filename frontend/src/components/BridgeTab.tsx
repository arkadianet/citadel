import { useState, useEffect, useCallback } from 'react'
import {
  initBridgeConfig,
  getBridgeState,
  getBridgeTokens,
  getBridgeFees,
  chainDisplayName,
  addressPlaceholder,
  formatTokenAmount,
  type RosenBridgeState,
  type BridgeTokenInfo,
  type BridgeFeeInfo,
} from '../api/rosen'
import { BridgeModal } from './BridgeModal'
import './BridgeTab.css'

interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

interface BridgeTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

export function BridgeTab({
  isConnected,
  walletAddress,
  walletBalance,
  explorerUrl,
}: BridgeTabProps) {
  const [bridgeState, setBridgeState] = useState<RosenBridgeState | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [initialized, setInitialized] = useState(false)

  // Form state
  const [selectedChain, setSelectedChain] = useState<string>('')
  const [chainTokens, setChainTokens] = useState<BridgeTokenInfo[]>([])
  const [selectedToken, setSelectedToken] = useState<BridgeTokenInfo | null>(null)
  const [amount, setAmount] = useState('')
  const [targetAddress, setTargetAddress] = useState('')

  // Fee state
  const [fees, setFees] = useState<BridgeFeeInfo | null>(null)
  const [feesLoading, setFeesLoading] = useState(false)
  const [feeError, setFeeError] = useState<string | null>(null)

  // Modal state
  const [modalOpen, setModalOpen] = useState(false)

  // Initialize bridge config on mount
  const initialize = useCallback(async () => {
    if (!isConnected || initialized) return
    setLoading(true)
    setError(null)
    try {
      await initBridgeConfig()
      const state = await getBridgeState()
      setBridgeState(state)
      setInitialized(true)
      if (state.supportedChains.length > 0) {
        setSelectedChain(state.supportedChains[0])
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, initialized])

  useEffect(() => {
    initialize()
  }, [initialize])

  // Update tokens when chain changes
  useEffect(() => {
    if (!selectedChain || !initialized) return
    getBridgeTokens(selectedChain)
      .then(tokens => {
        setChainTokens(tokens)
        setSelectedToken(tokens.length > 0 ? tokens[0] : null)
        setFees(null)
      })
      .catch(e => console.error('Failed to get bridge tokens:', e))
  }, [selectedChain, initialized])

  // Fetch fees when amount/token changes (debounced)
  useEffect(() => {
    if (!selectedToken || !amount || !selectedChain) {
      setFees(null)
      return
    }

    const amountNum = parseFloat(amount)
    if (isNaN(amountNum) || amountNum <= 0) {
      setFees(null)
      return
    }

    const baseAmount = Math.floor(amountNum * Math.pow(10, selectedToken.decimals))
    if (baseAmount <= 0) return

    const timeout = setTimeout(async () => {
      setFeesLoading(true)
      setFeeError(null)
      try {
        const result = await getBridgeFees(selectedToken.ergoTokenId, selectedChain, baseAmount)
        setFees(result)
      } catch (e) {
        setFeeError(String(e))
        setFees(null)
      } finally {
        setFeesLoading(false)
      }
    }, 500)

    return () => clearTimeout(timeout)
  }, [amount, selectedToken, selectedChain])

  const handleMaxClick = () => {
    if (!selectedToken || !walletBalance) return

    if (selectedToken.ergoTokenId === 'erg') {
      // Leave room for fees (miner fee + min box value)
      const maxNano = walletBalance.erg_nano - 5_000_000
      if (maxNano > 0) {
        setAmount((maxNano / 1e9).toString())
      }
    } else {
      const token = walletBalance.tokens.find(t => t.token_id === selectedToken.ergoTokenId)
      if (token) {
        const divisor = Math.pow(10, selectedToken.decimals)
        setAmount((token.amount / divisor).toString())
      }
    }
  }

  const getUserBalance = (): string => {
    if (!walletBalance || !selectedToken) return '0'
    if (selectedToken.ergoTokenId === 'erg') {
      return walletBalance.erg_formatted
    }
    const token = walletBalance.tokens.find(t => t.token_id === selectedToken.ergoTokenId)
    if (!token) return '0'
    return formatTokenAmount(token.amount, selectedToken.decimals)
  }

  const canBridge = () => {
    if (!selectedToken || !selectedChain || !amount || !targetAddress || !fees) return false
    const amountNum = parseFloat(amount)
    if (isNaN(amountNum) || amountNum <= 0) return false
    const baseAmount = Math.floor(amountNum * Math.pow(10, selectedToken.decimals))
    const minTransfer = parseInt(fees.minTransfer)
    if (baseAmount < minTransfer) return false
    return parseInt(fees.receivingAmount) > 0
  }

  const handleBridge = () => {
    setModalOpen(true)
  }

  if (!isConnected) {
    return (
      <div className="bridge-tab">
        <div className="bridge-header">
          <div className="bridge-header-row">
            <div className="protocol-app-icon" style={{ background: 'linear-gradient(135deg, #f97316, #8b5cf6)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="24" height="24">
                <path d="M4 12h6m4 0h6M10 12a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm4 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4z" />
              </svg>
            </div>
            <div>
              <h2>Rosen Bridge</h2>
              <p className="bridge-description">Cross-chain bridging from Ergo</p>
            </div>
          </div>
        </div>
        <div className="empty-state">
          <p>Connect to a node to use the bridge</p>
        </div>
      </div>
    )
  }

  return (
    <div className="bridge-tab">
      {/* Header */}
      <div className="bridge-header">
        <div className="bridge-header-row">
          <div className="protocol-app-icon" style={{ background: 'linear-gradient(135deg, #f97316, #8b5cf6)' }}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="24" height="24">
              <path d="M4 12h6m4 0h6M10 12a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm4 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4z" />
            </svg>
          </div>
          <div>
            <h2>Rosen Bridge</h2>
            <p className="bridge-description">Bridge tokens from Ergo to other chains</p>
          </div>
        </div>
      </div>

      {/* Development warning */}
      <div className="bridge-dev-warning">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
        <div>
          <strong>Experimental</strong> — This integration is still in development. Use at your own risk. In the worst case, funds sent through this interface could be lost.
        </div>
      </div>

      {/* Info bar */}
      {bridgeState && (
        <div className="protocol-info-bar">
          <div className="info-item">
            <span className="info-label">Chains:</span>
            <span className="info-value">{bridgeState.supportedChains.length}</span>
          </div>
          <div className="info-divider" />
          <div className="info-item">
            <span className="info-label">Tokens:</span>
            <span className="info-value">{bridgeState.availableTokens.length}</span>
          </div>
          <div className="info-status">
            <span className="dot" />
            <span className="info-value">Active</span>
          </div>
        </div>
      )}

      {loading && (
        <div className="bridge-loading">
          <div className="spinner-small" />
          <p>Loading bridge configuration...</p>
        </div>
      )}

      {error && (
        <div className="bridge-error">
          <p>{error}</p>
          <button className="btn btn-secondary" onClick={() => { setInitialized(false); setError(null) }}>
            Retry
          </button>
        </div>
      )}

      {initialized && !loading && (
        <div className="bridge-form-container">
          <div className="bridge-form">
            {/* Target Chain */}
            <div className="bridge-form-group">
              <label className="bridge-form-label">Destination Chain</label>
              <select
                className="bridge-select"
                value={selectedChain}
                onChange={e => setSelectedChain(e.target.value)}
              >
                {bridgeState?.supportedChains.map(chain => (
                  <option key={chain} value={chain}>
                    {chainDisplayName(chain)}
                  </option>
                ))}
              </select>
            </div>

            {/* Token */}
            <div className="bridge-form-group">
              <label className="bridge-form-label">Token</label>
              <select
                className="bridge-select"
                value={selectedToken?.ergoTokenId ?? ''}
                onChange={e => {
                  const token = chainTokens.find(t => t.ergoTokenId === e.target.value)
                  setSelectedToken(token ?? null)
                }}
              >
                {chainTokens.map(token => (
                  <option key={token.ergoTokenId} value={token.ergoTokenId}>
                    {token.name || token.ergoTokenId.slice(0, 8)}
                  </option>
                ))}
              </select>
              {selectedToken && (
                <div className="bridge-balance-hint">
                  Balance: {getUserBalance()} {selectedToken.name}
                </div>
              )}
            </div>

            {/* Amount */}
            <div className="bridge-form-group">
              <label className="bridge-form-label">Amount</label>
              <div className="bridge-input-wrapper">
                <input
                  type="text"
                  className="bridge-input"
                  placeholder="0.0"
                  value={amount}
                  onChange={e => setAmount(e.target.value)}
                />
                <button className="bridge-max-btn" onClick={handleMaxClick}>
                  MAX
                </button>
              </div>
            </div>

            {/* Destination Address */}
            <div className="bridge-form-group">
              <label className="bridge-form-label">
                {chainDisplayName(selectedChain)} Address
              </label>
              <input
                type="text"
                className="bridge-input bridge-address-input"
                placeholder={addressPlaceholder(selectedChain)}
                value={targetAddress}
                onChange={e => setTargetAddress(e.target.value)}
              />
            </div>

            {/* Fee Display */}
            {feesLoading && (
              <div className="bridge-fees-loading">
                <div className="spinner-small" /> Fetching fees...
              </div>
            )}

            {feeError && (
              <div className="bridge-fee-error">{feeError}</div>
            )}

            {fees && selectedToken && (
              <div className="bridge-fees">
                <div className="bridge-fee-row">
                  <span>Bridge Fee</span>
                  <span>{formatTokenAmount(parseInt(fees.bridgeFee), selectedToken.decimals)} {selectedToken.name}</span>
                </div>
                <div className="bridge-fee-row">
                  <span>Network Fee</span>
                  <span>{formatTokenAmount(parseInt(fees.networkFee), selectedToken.decimals)} {selectedToken.name}</span>
                </div>
                {fees.feeRatioBps > 0 && (
                  <div className="bridge-fee-row">
                    <span>Variable Fee</span>
                    <span>{(fees.feeRatioBps / 100).toFixed(2)}%</span>
                  </div>
                )}
                <div className="bridge-fee-row highlight">
                  <span>You Receive</span>
                  <span>{formatTokenAmount(parseInt(fees.receivingAmount), selectedToken.decimals)} {selectedToken.name}</span>
                </div>
                {parseInt(fees.receivingAmount) <= 0 && (
                  <div className="bridge-fee-warning">
                    Amount too low — fees exceed transfer amount
                  </div>
                )}
              </div>
            )}

            {/* Bridge Button */}
            <button
              className="bridge-submit-btn"
              disabled={!canBridge()}
              onClick={handleBridge}
            >
              Bridge to {chainDisplayName(selectedChain)}
            </button>
          </div>
        </div>
      )}

      {/* Bridge Modal */}
      {modalOpen && selectedToken && fees && walletAddress && (
        <BridgeModal
          isOpen={modalOpen}
          onClose={() => setModalOpen(false)}
          token={selectedToken}
          amount={amount}
          targetChain={selectedChain}
          targetAddress={targetAddress}
          fees={fees}
          walletAddress={walletAddress}
          explorerUrl={explorerUrl}
          onSuccess={() => {
            setModalOpen(false)
            setAmount('')
            setTargetAddress('')
            setFees(null)
          }}
        />
      )}
    </div>
  )
}
