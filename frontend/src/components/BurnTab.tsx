import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { buildMultiBurnTx, startBurnSign, getBurnTxStatus } from '../api/burn'
import type { BurnItemInput, BurnedTokenEntry } from '../api/burn'
import { getCachedTokenInfo } from '../api/tokenCache'
import { formatTokenAmount } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import './BurnTab.css'

interface BurnTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
}

type BurnStep = 'select' | 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'

interface CartEntry {
  amount: string  // display value (user-typed)
  rawAmount: number  // raw integer amount
}

/** Generate a deterministic color from a token ID. */
function avatarColor(tokenId: string): string {
  const colors = [
    '#ef4444', '#f97316', '#f59e0b', '#84cc16', '#22c55e',
    '#14b8a6', '#06b6d4', '#3b82f6', '#6366f1', '#8b5cf6',
    '#a855f7', '#d946ef', '#ec4899', '#f43f5e',
  ]
  let hash = 0
  for (let i = 0; i < tokenId.length; i++) hash = (hash * 31 + tokenId.charCodeAt(i)) | 0
  return colors[Math.abs(hash) % colors.length]
}

export function BurnTab({ isConnected, walletAddress, walletBalance, explorerUrl }: BurnTabProps) {
  const [burnCart, setBurnCart] = useState<Map<string, CartEntry>>(new Map())
  const [search, setSearch] = useState('')
  const [step, setStep] = useState<BurnStep>('select')
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)
  const [resolvedNames, setResolvedNames] = useState<Map<string, string>>(new Map())
  const [multiBurnSummary, setMultiBurnSummary] = useState<{
    burnedTokens: BurnedTokenEntry[]
    minerFee: number
  } | null>(null)

  const tokens = walletBalance?.tokens ?? []

  // Resolve names for tokens that have null names
  useEffect(() => {
    const unknown = tokens.filter(t => !t.name)
    if (unknown.length === 0) return

    let cancelled = false
    for (const t of unknown) {
      getCachedTokenInfo(t.token_id)
        .then(info => {
          if (cancelled) return
          if (info.name) {
            setResolvedNames(prev => {
              const next = new Map(prev)
              next.set(t.token_id, info.name!)
              return next
            })
          }
        })
        .catch(() => {})
    }
    return () => { cancelled = true }
  }, [tokens])

  /** Get the display name for a token. */
  const getTokenName = useCallback((t: { token_id: string; name: string | null }): string =>
    t.name || resolvedNames.get(t.token_id) || t.token_id.slice(0, 8) + '...', [resolvedNames])

  // Filter tokens by search
  const filteredTokens = useMemo(() => {
    if (!search.trim()) return tokens
    const q = search.toLowerCase()
    return tokens.filter(t => {
      const name = getTokenName(t).toLowerCase()
      return name.includes(q) || t.token_id.toLowerCase().includes(q)
    })
  }, [tokens, search, getTokenName])

  // Reset on wallet change
  useEffect(() => {
    setStep('select')
    setBurnCart(new Map())
    setError(null)
    setSearch('')
  }, [walletAddress])

  // Poll for tx status
  useEffect(() => {
    if (step !== 'signing' || !requestId) return

    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await getBurnTxStatus(requestId)
        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setStep('success')
        } else if (status.status === 'failed' || status.status === 'expired') {
          setError(status.error || 'Transaction failed')
          setStep('error')
        }
      } catch (e) {
        console.error('Poll error:', e)
      } finally {
        isPolling = false
      }
    }

    const interval = setInterval(poll, 2000)
    return () => clearInterval(interval)
  }, [step, requestId])

  // Cart operations
  const toggleToken = (tokenId: string) => {
    setBurnCart(prev => {
      const next = new Map(prev)
      if (next.has(tokenId)) {
        next.delete(tokenId)
      } else {
        const token = tokens.find(t => t.token_id === tokenId)
        if (token) {
          next.set(tokenId, {
            amount: formatTokenAmount(token.amount, token.decimals),
            rawAmount: token.amount,
          })
        }
      }
      return next
    })
  }

  const selectAll = () => {
    const next = new Map<string, CartEntry>()
    for (const t of filteredTokens) {
      next.set(t.token_id, {
        amount: formatTokenAmount(t.amount, t.decimals),
        rawAmount: t.amount,
      })
    }
    setBurnCart(next)
  }

  const deselectAll = () => {
    setBurnCart(new Map())
  }

  const removeFromCart = (tokenId: string) => {
    setBurnCart(prev => {
      const next = new Map(prev)
      next.delete(tokenId)
      return next
    })
  }

  const updateCartAmount = (tokenId: string, displayAmount: string) => {
    const token = tokens.find(t => t.token_id === tokenId)
    if (!token) return
    const parsed = parseFloat(displayAmount.replace(/,/g, ''))
    const raw = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, token.decimals))
    setBurnCart(prev => {
      const next = new Map(prev)
      next.set(tokenId, { amount: displayAmount, rawAmount: raw })
      return next
    })
  }

  const setMaxAmount = (tokenId: string) => {
    const token = tokens.find(t => t.token_id === tokenId)
    if (!token) return
    setBurnCart(prev => {
      const next = new Map(prev)
      next.set(tokenId, {
        amount: formatTokenAmount(token.amount, token.decimals),
        rawAmount: token.amount,
      })
      return next
    })
  }

  // Validation
  const cartIsValid = useMemo(() => {
    if (burnCart.size === 0) return false
    for (const [tokenId, entry] of burnCart) {
      if (entry.rawAmount <= 0) return false
      const token = tokens.find(t => t.token_id === tokenId)
      if (!token || entry.rawAmount > token.amount) return false
    }
    return true
  }, [burnCart, tokens])

  const handleConfirm = () => {
    if (!cartIsValid) {
      setError('Fix invalid amounts in the basket')
      return
    }
    setError(null)
    setStep('confirm')
  }

  const handleBurn = async () => {
    setLoading(true)
    setError(null)
    setStep('building')

    try {
      const utxos = await invoke<Array<{ ergo_tree?: string; ergoTree?: string }>>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const userErgoTree = utxos[0].ergo_tree || utxos[0].ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const burnItems: BurnItemInput[] = []
      for (const [tokenId, entry] of burnCart) {
        burnItems.push({ tokenId, amount: entry.rawAmount })
      }

      const result = await buildMultiBurnTx(
        burnItems,
        userErgoTree,
        utxos as object[],
        nodeStatus.chain_height,
      )

      setMultiBurnSummary({
        burnedTokens: result.burnedTokens,
        minerFee: result.minerFee,
      })

      const count = burnItems.length
      const message = `Burn ${count} token${count !== 1 ? 's' : ''}`
      const signResult = await startBurnSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const handleNautilusSign = async () => {
    if (!nautilusUrl) return
    setSignMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      setError(String(e))
    }
  }

  const handleReset = () => {
    setStep('select')
    setBurnCart(new Map())
    setError(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setRequestId(null)
    setTxId(null)
    setSignMethod('choose')
    setMultiBurnSummary(null)
    setSearch('')
  }

  // Empty states
  if (!isConnected || !walletAddress) {
    return (
      <div className="burn-tab">
        <div className="burn-header">
          <h2>Token Burn</h2>
          <p className="burn-description">Permanently destroy tokens by removing them from circulation.</p>
        </div>
        <div className="message warning">
          {!isConnected ? 'Connect to a node to use the burn tool.' : 'Connect your wallet to burn tokens.'}
        </div>
      </div>
    )
  }

  // Main select step — token list + basket side by side
  if (step === 'select') {
    return (
      <div className="burn-tab">
        <div className="burn-header">
          <h2>Token Burn</h2>
          <p className="burn-description">Permanently destroy tokens by removing them from circulation. Select multiple tokens to burn in a single transaction.</p>
        </div>

        <div className="burn-layout">
          {/* Token picker */}
          <div className="burn-token-panel">
            <div className="burn-token-toolbar">
              <div className="burn-toolbar-actions">
                <button className="burn-toolbar-btn" onClick={selectAll}>Select All</button>
                <button className="burn-toolbar-btn" onClick={deselectAll}>Deselect All</button>
              </div>
              {burnCart.size > 0 && (
                <span className="burn-cart-badge">{burnCart.size}</span>
              )}
            </div>

            <div className="burn-token-search">
              <input
                type="text"
                placeholder="Search tokens..."
                value={search}
                onChange={e => setSearch(e.target.value)}
              />
            </div>

            <div className="burn-token-list">
              {filteredTokens.length === 0 ? (
                <div className="burn-token-empty">
                  {tokens.length === 0 ? (
                    <span>No tokens in wallet</span>
                  ) : (
                    <span>No tokens match "{search}"</span>
                  )}
                </div>
              ) : (
                filteredTokens.map(t => {
                  const name = getTokenName(t)
                  const inCart = burnCart.has(t.token_id)
                  return (
                    <button
                      key={t.token_id}
                      className={`burn-token-item${inCart ? ' in-cart' : ''}`}
                      onClick={() => toggleToken(t.token_id)}
                    >
                      <div className="burn-token-avatar-wrap">
                        <div
                          className="burn-token-avatar"
                          style={{ background: avatarColor(t.token_id) }}
                        >
                          {name.charAt(0).toUpperCase()}
                        </div>
                        {inCart && (
                          <div className="burn-token-check">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3">
                              <polyline points="20 6 9 17 4 12" />
                            </svg>
                          </div>
                        )}
                      </div>
                      <div className="burn-token-info">
                        <span className="burn-token-name">{name}</span>
                        <span className="burn-token-id">{t.token_id.slice(0, 16)}...</span>
                      </div>
                      <span className="burn-token-balance">
                        {formatTokenAmount(t.amount, t.decimals)}
                      </span>
                    </button>
                  )
                })
              )}
            </div>

            {tokens.length > 0 && (
              <div className="burn-token-count">
                {filteredTokens.length} of {tokens.length} token{tokens.length !== 1 ? 's' : ''}
              </div>
            )}
          </div>

          {/* Basket panel */}
          <div className="burn-form-panel">
            {burnCart.size === 0 ? (
              <div className="burn-form-empty">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z" />
                  <path d="M8 12h8M12 8v8" />
                </svg>
                <span>Select tokens to burn</span>
              </div>
            ) : (
              <>
                <div className="burn-basket-list">
                  {Array.from(burnCart.entries()).map(([tokenId, entry]) => {
                    const token = tokens.find(t => t.token_id === tokenId)
                    if (!token) return null
                    const name = getTokenName(token)
                    const overBalance = entry.rawAmount > token.amount
                    return (
                      <div key={tokenId} className="burn-basket-item">
                        <div className="burn-basket-item-header">
                          <div
                            className="burn-token-avatar burn-basket-avatar"
                            style={{ background: avatarColor(tokenId) }}
                          >
                            {name.charAt(0).toUpperCase()}
                          </div>
                          <span className="burn-basket-name">{name}</span>
                          <button
                            className="burn-basket-remove"
                            onClick={() => removeFromCart(tokenId)}
                            title="Remove"
                          >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <path d="M18 6L6 18M6 6l12 12" />
                            </svg>
                          </button>
                        </div>
                        <div className="burn-basket-amount-row">
                          <div className="burn-basket-amount-wrapper">
                            <input
                              type="text"
                              inputMode="decimal"
                              value={entry.amount}
                              onChange={e => updateCartAmount(tokenId, e.target.value)}
                              className={overBalance ? 'over-balance' : ''}
                            />
                            <button
                              className="burn-max-btn"
                              onClick={() => setMaxAmount(tokenId)}
                            >
                              Max
                            </button>
                          </div>
                          <span className="burn-basket-balance">
                            of {formatTokenAmount(token.amount, token.decimals)}
                          </span>
                        </div>
                      </div>
                    )
                  })}
                </div>

                {error && <div className="message error">{error}</div>}

                <div className="burn-basket-footer">
                  <div className="burn-basket-footer-info">
                    <span>{burnCart.size} token{burnCart.size !== 1 ? 's' : ''}</span>
                    <span className="burn-basket-fee">Fee: ~0.0011 ERG</span>
                  </div>
                  <button
                    className="burn-submit-btn"
                    onClick={handleConfirm}
                    disabled={!cartIsValid}
                  >
                    Review Burn
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    )
  }

  // Confirm step
  if (step === 'confirm') {
    const cartEntries = Array.from(burnCart.entries())
    return (
      <div className="burn-tab">
        <div className="burn-header">
          <h2>Confirm Token Burn</h2>
        </div>
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="burn-confirm-token-list">
                {cartEntries.map(([tokenId, entry]) => {
                  const token = tokens.find(t => t.token_id === tokenId)
                  if (!token) return null
                  const name = getTokenName(token)
                  return (
                    <div key={tokenId} className="burn-confirm-row burn-amount-row">
                      <span className="burn-confirm-token-name">
                        <div
                          className="burn-token-avatar burn-confirm-avatar"
                          style={{ background: avatarColor(tokenId) }}
                        >
                          {name.charAt(0).toUpperCase()}
                        </div>
                        {name}
                      </span>
                      <span>{formatTokenAmount(entry.rawAmount, token.decimals)}</span>
                    </div>
                  )
                })}
              </div>

              <div className="burn-confirm-summary" style={{ marginTop: 'var(--space-md)' }}>
                <div className="burn-confirm-row">
                  <span>Tokens</span>
                  <span>{burnCart.size}</span>
                </div>
                <div className="burn-confirm-row">
                  <span>Miner Fee</span>
                  <span>~0.0011 ERG</span>
                </div>
              </div>

              <div className="burn-danger-box" style={{ marginTop: 'var(--space-md)' }}>
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--red-400)" strokeWidth="2">
                  <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                  <line x1="12" y1="9" x2="12" y2="13" />
                  <line x1="12" y1="17" x2="12.01" y2="17" />
                </svg>
                <p>This action is <strong>IRREVERSIBLE</strong>. These tokens will be permanently destroyed.</p>
              </div>

              <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                <button
                  className="btn btn-primary"
                  style={{ background: 'var(--red-500)' }}
                  onClick={handleBurn}
                  disabled={loading}
                >
                  {loading ? 'Building...' : `Burn ${burnCart.size} Token${burnCart.size !== 1 ? 's' : ''} Forever`}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Building step
  if (step === 'building') {
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="swap-preview-loading">
                <div className="spinner-small" />
                <span>Building burn transaction...</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Signing step — choose method
  if (step === 'signing' && signMethod === 'choose') {
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                {multiBurnSummary && (
                  <div className="burn-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="burn-confirm-row burn-amount-row">
                      <span>Burning</span>
                      <span>{multiBurnSummary.burnedTokens.length} token{multiBurnSummary.burnedTokens.length !== 1 ? 's' : ''}</span>
                    </div>
                    <div className="burn-confirm-row">
                      <span>Miner Fee</span>
                      <span>{(multiBurnSummary.minerFee / 1e9).toFixed(4)} ERG</span>
                    </div>
                  </div>
                )}
                <p>Choose your signing method</p>
                <div className="wallet-options">
                  <button className="wallet-option" onClick={handleNautilusSign}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="2" y="3" width="20" height="14" rx="2" />
                        <path d="M8 21h8" /><path d="M12 17v4" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Nautilus Extension</span>
                      <span className="wallet-option-desc">Sign with browser extension</span>
                    </div>
                  </button>
                  <button className="wallet-option" onClick={() => setSignMethod('mobile')}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="5" y="2" width="14" height="20" rx="2" />
                        <line x1="12" y1="18" x2="12.01" y2="18" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Mobile Wallet</span>
                      <span className="wallet-option-desc">Scan QR code with Ergo Wallet</span>
                    </div>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Signing step — Nautilus waiting
  if (step === 'signing' && signMethod === 'nautilus') {
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Approve the burn in Nautilus</p>
                <div className="nautilus-waiting">
                  <div className="nautilus-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" /><path d="M12 17v4" />
                    </svg>
                  </div>
                  <p className="signing-hint">Waiting for Nautilus approval...</p>
                </div>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
                  <button className="btn btn-primary" onClick={handleNautilusSign}>Open Nautilus Again</button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Signing step — QR code
  if (step === 'signing' && signMethod === 'mobile' && qrUrl) {
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Scan with your Ergo wallet to sign</p>
                <div className="qr-container">
                  <QRCodeSVG value={qrUrl} size={200} />
                </div>
                <p className="signing-hint">Waiting for signature...</p>
                <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Success
  if (step === 'success') {
    const count = multiBurnSummary?.burnedTokens.length ?? burnCart.size
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="success-step">
                <div className="success-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                  </svg>
                </div>
                <h3>{count} Token{count !== 1 ? 's' : ''} Burned!</h3>
                {multiBurnSummary && (
                  <div className="burn-success-list">
                    {multiBurnSummary.burnedTokens.map(bt => {
                      const token = tokens.find(t => t.token_id === bt.tokenId)
                      const name = token ? getTokenName(token) : bt.tokenId.slice(0, 8) + '...'
                      const decimals = token?.decimals ?? 0
                      return (
                        <div key={bt.tokenId} className="burn-success-item">
                          <span>{name}</span>
                          <span>{formatTokenAmount(bt.amount, decimals)}</span>
                        </div>
                      )
                    })}
                  </div>
                )}
                {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
                <button className="btn btn-primary" onClick={handleReset}>Burn More</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Error
  if (step === 'error') {
    return (
      <div className="burn-tab">
        <div className="burn-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="error-step">
                <div className="error-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                  </svg>
                </div>
                <h3>Burn Failed</h3>
                <p className="error-message">{error}</p>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={handleReset}>Start Over</button>
                  <button className="btn btn-primary" onClick={() => { setStep('confirm'); setError(null) }}>
                    Try Again
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return null
}
