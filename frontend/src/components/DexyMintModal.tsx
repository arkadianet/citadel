import { useState, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import './DexyMintModal.css'
import type { MintPath } from './DexyPathCard'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { TX_FEE_NANO } from '../constants'
import { AdvancedOptions, useRecipientAddress } from './AdvancedOptions'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'

interface DexyState {
  variant: string
  oracle_rate_nano: number
  lp_rate_nano: number
  dexy_in_bank: number
  can_mint: boolean
}

interface MintPaths {
  arb_mint: MintPath
  free_mint: MintPath
  lp_swap: MintPath
}

interface DexyRates {
  variant: string
  token_name: string
  token_decimals: number
  oracle_rate_nano: number
  erg_per_token: number
  tokens_per_erg: number
  peg_description: string
  paths: MintPaths
}

type PathType = 'free_mint' | 'lp_swap'

interface PreviewResponse {
  erg_cost_nano: string
  tx_fee_nano: string
  total_cost_nano: string
  token_amount: string
  token_name: string
  can_execute: boolean
  error: string | null
}

interface DexyMintModalProps {
  isOpen: boolean
  onClose: () => void
  variant: 'gold' | 'usd'
  state: DexyState | null
  walletAddress: string
  ergBalance: number
  explorerUrl: string
  onSuccess: () => void
}

type TxStep = 'input' | 'preview' | 'signing' | 'success' | 'error'

const VARIANT_CONFIG = {
  gold: {
    name: 'DexyGold',
    decimals: 0,
    color: '#fbbf24',
  },
  usd: {
    name: 'USE',
    decimals: 3,  // USE has 3 decimals, not 6
    color: '#22c55e',
  },
}

function pollMintStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}

export function DexyMintModal({
  isOpen,
  onClose,
  variant,
  state,
  walletAddress,
  ergBalance,
  explorerUrl,
  onSuccess,
}: DexyMintModalProps) {
  const config = VARIANT_CONFIG[variant]
  const { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } = useRecipientAddress()
  const [step, setStep] = useState<TxStep>('input')
  const [ergInput, setErgInput] = useState('')
  const [tokenInput, setTokenInput] = useState('')
  const [lastEdited, setLastEdited] = useState<'erg' | 'token'>('token')
  const [preview, setPreview] = useState<PreviewResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: pollMintStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'Dexy', operation: 'mint', description: `Dexy ${variant} mint` },
  })

  // Path selection state
  const [rates, setRates] = useState<DexyRates | null>(null)
  const [ratesLoading, setRatesLoading] = useState(false)
  const [ratesError, setRatesError] = useState<string | null>(null)
  const [selectedPath, setSelectedPath] = useState<PathType | null>(null)

  // Get the effective rate for the selected path
  const effectiveRate = useMemo(() => {
    if (!rates || !selectedPath) return null
    const path = rates.paths[selectedPath]
    // Use effective_rate (includes fees) or erg_per_token
    return path.effective_rate || path.erg_per_token || null
  }, [rates, selectedPath])

  // Get the selected path data
  const selectedPathData = useMemo(() => {
    if (!rates || !selectedPath) return null
    return rates.paths[selectedPath]
  }, [rates, selectedPath])

  // Calculate amounts in real-time based on which field was last edited
  const calculated = useMemo(() => {
    const activeInput = lastEdited === 'erg' ? ergInput : tokenInput
    if (!state || !activeInput || !effectiveRate) {
      return { tokenAmount: 0, ergCost: 0, totalCost: 0, isValid: false }
    }

    const value = parseFloat(activeInput)
    if (isNaN(value) || value <= 0) {
      return { tokenAmount: 0, ergCost: 0, totalCost: 0, isValid: false }
    }

    const tokenMultiplier = Math.pow(10, config.decimals)

    let tokenAmount: number
    let ergCost: number

    if (lastEdited === 'token') {
      tokenAmount = value
      ergCost = value * effectiveRate
    } else {
      ergCost = value
      tokenAmount = value / effectiveRate
      // Round down to avoid exceeding user's ERG
      tokenAmount = Math.floor(tokenAmount * tokenMultiplier) / tokenMultiplier
    }

    const totalCost = ergCost + (TX_FEE_NANO / 1e9)
    const tokenAmountRaw = Math.round(tokenAmount * tokenMultiplier)

    // Validation
    const hasEnoughErg = totalCost <= ergBalance / 1e9
    const hasEnoughInBank = tokenAmountRaw <= state.dexy_in_bank
    const isPositive = tokenAmount > 0 && ergCost > 0

    // Check path-specific limits
    let hasEnoughInPath = true
    if (selectedPathData?.max_tokens !== undefined) {
      hasEnoughInPath = tokenAmountRaw <= selectedPathData.max_tokens
    }
    if (selectedPathData?.remaining_today !== undefined) {
      hasEnoughInPath = hasEnoughInPath && tokenAmountRaw <= selectedPathData.remaining_today
    }

    return {
      tokenAmount,
      tokenAmountRaw,
      ergCost,
      totalCost,
      isValid: isPositive && hasEnoughErg && hasEnoughInBank && hasEnoughInPath && selectedPath !== null,
      hasEnoughErg,
      hasEnoughInBank,
      hasEnoughInPath,
    }
  }, [ergInput, tokenInput, lastEdited, state, config.decimals, ergBalance, effectiveRate, selectedPath, selectedPathData])

  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setErgInput('')
      setTokenInput('')
      setLastEdited('token')
      setPreview(null)
      setError(null)
      setSelectedPath('free_mint')
      setRates(null)
      setRatesError(null)
      setRecipientAddress('')
      // Fetch rates for FreeMint path
      setRatesLoading(true)
      invoke<DexyRates>('get_dexy_rates', { variant })
        .then((fetchedRates) => {
          setRates(fetchedRates)
          setSelectedPath('free_mint')
        })
        .catch((e) => {
          setRatesError(String(e))
        })
        .finally(() => {
          setRatesLoading(false)
        })
    }
  }, [isOpen, variant])

  const handlePreview = async () => {
    if (!calculated.isValid || !calculated.tokenAmountRaw) {
      setError('Please enter a valid amount')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const result = await invoke<PreviewResponse>('preview_mint_dexy', {
        request: { variant, amount: calculated.tokenAmountRaw, user_address: walletAddress }
      })

      setPreview(result)
      if (result.can_execute) {
        setStep('preview')
      } else {
        setError(result.error || 'Cannot execute')
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const handleSign = async () => {
    if (!preview || !calculated.tokenAmountRaw) return

    setLoading(true)
    setError(null)

    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const buildResult = await invoke<{ unsigned_tx: unknown }>('build_mint_dexy', {
        request: {
          variant,
          amount: calculated.tokenAmountRaw,
          user_address: walletAddress,
          user_utxos: utxos,
          current_height: nodeStatus.chain_height,
          recipient_address: recipientOrNull,
        }
      })

      const signResult = await invoke<{
        request_id: string
        ergopay_url: string
        nautilus_url: string
      }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `Mint ${calculated.tokenAmount.toFixed(config.decimals)} ${config.name}`,
        }
      })

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const formatToken = (amount: number) => {
    if (config.decimals === 0) {
      return amount.toLocaleString(undefined, { maximumFractionDigits: 0 })
    }
    return amount.toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: config.decimals,
    })
  }

  const handleErgChange = (value: string) => {
    setErgInput(value)
    setLastEdited('erg')
    if (effectiveRate) {
      const parsed = parseFloat(value)
      if (!isNaN(parsed) && parsed > 0) {
        const tokenMultiplier = Math.pow(10, config.decimals)
        const tokenValue = Math.floor((parsed / effectiveRate) * tokenMultiplier) / tokenMultiplier
        setTokenInput(config.decimals === 0 ? Math.floor(tokenValue).toString() : tokenValue.toFixed(config.decimals))
      } else {
        setTokenInput('')
      }
    }
  }

  const handleTokenChange = (value: string) => {
    setTokenInput(value)
    setLastEdited('token')
    if (effectiveRate) {
      const parsed = parseFloat(value)
      if (!isNaN(parsed) && parsed > 0) {
        setErgInput((parsed * effectiveRate).toFixed(4))
      } else {
        setErgInput('')
      }
    }
  }

  const handleMaxClick = () => {
    if (!state || !effectiveRate || !selectedPathData) return

    const availableErg = (ergBalance / 1e9) - (TX_FEE_NANO / 1e9) - 0.001 // Leave small buffer
    const tokenMultiplier = Math.pow(10, config.decimals)

    // Calculate max based on ERG balance using the effective rate
    const maxFromErg = availableErg / effectiveRate
    const maxFromBank = state.dexy_in_bank / tokenMultiplier

    // Also consider path-specific limits
    let maxFromPath = maxFromBank
    if (selectedPathData.max_tokens !== undefined) {
      maxFromPath = Math.min(maxFromPath, selectedPathData.max_tokens / tokenMultiplier)
    }
    if (selectedPathData.remaining_today !== undefined) {
      maxFromPath = Math.min(maxFromPath, selectedPathData.remaining_today / tokenMultiplier)
    }

    const maxToken = Math.min(maxFromErg, maxFromBank, maxFromPath)
    const ergForMax = maxToken * effectiveRate

    setTokenInput(config.decimals === 0 ? Math.floor(maxToken).toString() : maxToken.toFixed(config.decimals))
    setErgInput(ergForMax.toFixed(4))
    setLastEdited('erg')
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal dexy-mint-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Mint {config.name}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="mint-input-step">
              {ratesLoading && (
                <div className="path-toggle-loading">
                  <div className="spinner-small" /> Loading rate...
                </div>
              )}
              {ratesError && (
                <div className="path-cards-error">
                  Failed to load rate: {ratesError}
                </div>
              )}

              {/* Swap Inputs */}
              <div className="swap-inputs">
                {/* You Pay (ERG) */}
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Pay</span>
                    <span className="swap-field-balance">
                      Balance: {formatErg(ergBalance)} ERG
                    </span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={ergInput}
                      onChange={e => handleErgChange(e.target.value)}
                      placeholder="0"
                      min="0"
                      step="0.0001"
                      disabled={!selectedPath}
                    />
                    <div className="swap-field-token">
                      <span>ERG</span>
                      <button className="max-btn" onClick={handleMaxClick} type="button" disabled={!selectedPath}>
                        MAX
                      </button>
                    </div>
                  </div>
                </div>

                {/* Arrow separator */}
                <div className="swap-arrow">&darr;</div>

                {/* You Receive (Token) */}
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Receive</span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={tokenInput}
                      onChange={e => handleTokenChange(e.target.value)}
                      placeholder="0"
                      min="0"
                      step={Math.pow(10, -config.decimals)}
                      disabled={!selectedPath}
                    />
                    <span className="swap-field-token">{config.name}</span>
                  </div>
                </div>
              </div>

              {/* Validation Warnings */}
              {(ergInput || tokenInput) && calculated.tokenAmount > 0 && (
                <>
                  {!calculated.hasEnoughErg && (
                    <div className="message warning">
                      Insufficient ERG balance (need {calculated.totalCost.toFixed(4)} ERG)
                    </div>
                  )}
                  {!calculated.hasEnoughInBank && (
                    <div className="message warning">
                      Exceeds available in bank ({state ? (state.dexy_in_bank / Math.pow(10, config.decimals)).toLocaleString() : 0} {config.name})
                    </div>
                  )}
                  {!calculated.hasEnoughInPath && selectedPathData && (
                    <div className="message warning">
                      Exceeds daily mint limit
                      {selectedPathData.remaining_today !== undefined && (
                        <> ({(selectedPathData.remaining_today / Math.pow(10, config.decimals)).toLocaleString()} available today)</>
                      )}
                    </div>
                  )}
                </>
              )}

              {selectedPath && selectedPathData && rates && (
                <div className="mint-info">
                  <div className="info-row">
                    <span>Rate</span>
                    <span>{effectiveRate?.toFixed(4)} ERG / {config.name}</span>
                  </div>
                  {state && (() => {
                    const tokenMultiplier = Math.pow(10, config.decimals)
                    const oracleRate = (state.oracle_rate_nano / 1e9) * tokenMultiplier
                    const swapRate = (state.lp_rate_nano / 1e9) * tokenMultiplier * 1.003
                    const mintBetter = oracleRate < swapRate
                    const savingPct = Math.abs(oracleRate - swapRate) / Math.max(oracleRate, swapRate) * 100
                    return savingPct > 0.1 ? (
                      <div className="info-row">
                        <span>vs LP Swap</span>
                        <span className={mintBetter ? 'positive' : 'negative'}>
                          {mintBetter
                            ? `Mint ${savingPct.toFixed(1)}% cheaper`
                            : `LP Swap ${savingPct.toFixed(1)}% cheaper`}
                        </span>
                      </div>
                    ) : null
                  })()}
                  {selectedPathData.remaining_today !== undefined && (
                    <div className="info-row">
                      <span>Available today</span>
                      <span>{(selectedPathData.remaining_today / Math.pow(10, config.decimals)).toLocaleString()} {config.name}</span>
                    </div>
                  )}
                  {selectedPathData.max_tokens !== undefined && selectedPathData.remaining_today === undefined && (
                    <div className="info-row">
                      <span>Max available</span>
                      <span>{(selectedPathData.max_tokens / Math.pow(10, config.decimals)).toLocaleString()} {config.name}</span>
                    </div>
                  )}
                  {selectedPathData.fee_percent > 0 && (
                    <div className="info-row">
                      <span>Protocol Fee</span>
                      <span>{selectedPathData.fee_percent}%</span>
                    </div>
                  )}
                  <div className="info-row">
                    <span>Transaction Fee</span>
                    <span>{(TX_FEE_NANO / 1e9).toFixed(4)} ERG</span>
                  </div>
                </div>
              )}

              <AdvancedOptions
                recipientAddress={recipientAddress}
                onRecipientChange={setRecipientAddress}
                addressValid={addressValid}
              />

              {error && <div className="message error">{error}</div>}

              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handlePreview}
                  disabled={loading || !calculated.isValid || (!!recipientAddress && addressValid !== true)}
                >
                  {loading ? 'Loading...' : 'Preview'}
                </button>
              </div>
            </div>
          )}

          {step === 'preview' && preview && (
            <div className="mint-preview-step">
              <div className="preview-summary">
                <div className="preview-header">
                  <span className="preview-label">You Will Receive</span>
                  <span className="preview-value">{formatToken(calculated.tokenAmount)} {config.name}</span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>Cost</span>
                    <span>{formatErg(Number(preview.erg_cost_nano))} ERG</span>
                  </div>
                  <div className="detail-row">
                    <span>Transaction Fee</span>
                    <span>{formatErg(Number(preview.tx_fee_nano))} ERG</span>
                  </div>
                  <div className="detail-row total">
                    <span>Total</span>
                    <span>{formatErg(Number(preview.total_cost_nano))} ERG</span>
                  </div>
                </div>

                {selectedPathData && selectedPathData.fee_percent > 0 ? (
                  <p className="preview-note">
                    Includes {selectedPathData.fee_percent}% protocol fee
                  </p>
                ) : (
                  <p className="preview-note">
                    No protocol fee for this path
                  </p>
                )}
              </div>

              {error && <div className="message error">{error}</div>}

              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={() => setStep('input')}>
                  Back
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleSign}
                  disabled={loading}
                >
                  {loading ? 'Building...' : 'Sign Transaction'}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && (
            <div className="mint-signing-step">
              {flow.signMethod === 'choose' && (
                <div className="sign-method-choice">
                  <p>Choose signing method:</p>
                  <div className="sign-methods">
                    <button className="sign-method-btn" onClick={flow.handleNautilusSign}>
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="2" y="3" width="20" height="14" rx="2" />
                        <path d="M8 21h8" />
                        <path d="M12 17v4" />
                      </svg>
                      <span>Nautilus</span>
                      <small>Browser Extension</small>
                    </button>
                    <button className="sign-method-btn" onClick={flow.handleMobileSign}>
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="5" y="2" width="14" height="20" rx="2" />
                        <line x1="12" y1="18" x2="12.01" y2="18" />
                      </svg>
                      <span>Mobile</span>
                      <small>Scan QR Code</small>
                    </button>
                  </div>
                </div>
              )}

              {flow.signMethod === 'nautilus' && (
                <div className="nautilus-waiting">
                  <div className="waiting-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" />
                      <path d="M12 17v4" />
                    </svg>
                  </div>
                  <p>Approve in Nautilus</p>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                    Back
                  </button>
                </div>
              )}

              {flow.signMethod === 'mobile' && flow.qrUrl && (
                <div className="qr-signing">
                  <p>Scan with Ergo Mobile Wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG
                      value={flow.qrUrl}
                      size={200}
                      level="M"
                      includeMargin
                      bgColor="white"
                      fgColor="black"
                    />
                  </div>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                    Back
                  </button>
                </div>
              )}
            </div>
          )}

          {step === 'success' && (
            <div className="mint-success-step">
              <div className="success-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--success)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Transaction Submitted!</h3>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={onSuccess}>
                Done
              </button>
            </div>
          )}

          {step === 'error' && (
            <div className="mint-error-step">
              <div className="error-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--error)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="15" y1="9" x2="9" y2="15" />
                  <line x1="9" y1="9" x2="15" y2="15" />
                </svg>
              </div>
              <h3>Transaction Failed</h3>
              <p className="error-message">{error}</p>
              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Close
                </button>
                <button className="btn btn-primary" onClick={() => setStep('input')}>
                  Try Again
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
