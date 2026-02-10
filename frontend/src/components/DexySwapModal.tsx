import { useState, useEffect, useRef, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  previewDexySwap,
  buildDexySwapTx,
  startDexySwapSign,
  getDexySwapTxStatus,
  type DexySwapPreviewResponse,
  type SwapDirection,
} from '../api/dexySwap'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { AdvancedOptions, useRecipientAddress } from './AdvancedOptions'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import './DexySwapModal.css'

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

interface DexySwapModalProps {
  isOpen: boolean
  onClose: () => void
  variant: 'gold' | 'usd'
  state: DexyState | null
  walletAddress: string
  ergBalance: number
  dexyBalance: number
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
    decimals: 3,
    color: '#22c55e',
  },
}

const SLIPPAGE_OPTIONS = [0.1, 0.5, 1.0]

export function DexySwapModal({
  isOpen,
  onClose,
  variant,
  state,
  walletAddress,
  ergBalance,
  dexyBalance,
  explorerUrl,
  onSuccess,
}: DexySwapModalProps) {
  const config = VARIANT_CONFIG[variant]
  const { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } = useRecipientAddress()

  // Step management
  const [step, setStep] = useState<TxStep>('input')
  const [direction, setDirection] = useState<SwapDirection>('erg_to_dexy')
  const [inputValue, setInputValue] = useState('')
  const [outputValue, setOutputValue] = useState('')
  const [lastEdited, setLastEdited] = useState<'input' | 'output'>('input')
  const [slippage, setSlippage] = useState(0.5)
  const [customSlippage, setCustomSlippage] = useState('')
  const [showCustomSlippage, setShowCustomSlippage] = useState(false)

  // Preview state
  const [preview, setPreview] = useState<DexySwapPreviewResponse | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)

  // Transaction state
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: getDexySwapTxStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'Dexy', operation: 'swap', description: `Dexy ${variant} swap` },
  })

  // Debounce timer ref
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setDirection('erg_to_dexy')
      setInputValue('')
      setOutputValue('')
      setLastEdited('input')
      setSlippage(0.5)
      setCustomSlippage('')
      setShowCustomSlippage(false)
      setPreview(null)
      setPreviewLoading(false)
      setPreviewError(null)
      setLoading(false)
      setError(null)
      setRecipientAddress('')
    }
  }, [isOpen])

  // Get the LP rate (ERG per token in display units) from state
  const lpRate = state ? (state.lp_rate_nano / 1e9) * Math.pow(10, config.decimals) : 0

  // Convert user display input to raw amount for the backend preview
  // Always returns the "input side" raw amount regardless of which field was edited
  const getAmountRaw = useCallback((): number | null => {
    if (lastEdited === 'input') {
      const parsed = parseFloat(inputValue)
      if (isNaN(parsed) || parsed <= 0) return null
      if (direction === 'erg_to_dexy') {
        return Math.round(parsed * 1e9)
      } else {
        return Math.round(parsed * Math.pow(10, config.decimals))
      }
    } else {
      // User typed in output field -- derive input from desired output using LP rate
      const parsed = parseFloat(outputValue)
      if (isNaN(parsed) || parsed <= 0 || !lpRate) return null
      if (direction === 'erg_to_dexy') {
        // Output is tokens, input is ERG: erg = tokens * rate
        const ergNeeded = parsed * lpRate * 1.005 // slight buffer for price impact
        return Math.round(ergNeeded * 1e9)
      } else {
        // Output is ERG, input is tokens: tokens = erg / rate
        const tokensNeeded = (parsed / lpRate) * 1.005
        return Math.round(tokensNeeded * Math.pow(10, config.decimals))
      }
    }
  }, [direction, config.decimals, inputValue, outputValue, lastEdited, lpRate])

  // Debounced preview fetch
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current)
    }

    const amountRaw = getAmountRaw()
    if (!amountRaw || !isOpen || step !== 'input') {
      setPreview(null)
      setPreviewError(null)
      return
    }

    setPreviewLoading(true)
    setPreviewError(null)

    debounceRef.current = setTimeout(async () => {
      try {
        const result = await previewDexySwap(variant, direction, amountRaw, slippage)
        setPreview(result)
        setPreviewError(null)

        // Update the non-edited field with the preview result
        if (lastEdited === 'input') {
          // Update output field from preview
          if (direction === 'erg_to_dexy') {
            setOutputValue(formatToken(result.output_amount, config.decimals))
          } else {
            setOutputValue(formatErg(result.output_amount))
          }
        } else {
          // Update input field from the amount we derived
          if (direction === 'erg_to_dexy') {
            setInputValue((amountRaw / 1e9).toFixed(4))
          } else {
            const divisor = Math.pow(10, config.decimals)
            setInputValue(config.decimals === 0
              ? Math.floor(amountRaw / divisor).toString()
              : (amountRaw / divisor).toFixed(config.decimals))
          }
        }
      } catch (e) {
        setPreviewError(String(e))
        setPreview(null)
      } finally {
        setPreviewLoading(false)
      }
    }, 500)

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current)
      }
    }
  }, [inputValue, outputValue, lastEdited, direction, slippage, variant, isOpen, step, getAmountRaw])

  // Get the max spendable amount for the current direction
  const maxAmount = (): string => {
    if (direction === 'erg_to_dexy') {
      // Leave buffer for miner fee
      const available = Math.max(0, (ergBalance / 1e9) - 0.0022)
      return available.toFixed(4)
    } else {
      // Max is user's dexy balance in display units
      const divisor = Math.pow(10, config.decimals)
      const available = dexyBalance / divisor
      return config.decimals === 0 ? Math.floor(available).toString() : available.toFixed(config.decimals)
    }
  }

  const handleMaxClick = () => {
    setInputValue(maxAmount())
    setLastEdited('input')
  }

  const handleDirectionChange = (newDirection: SwapDirection) => {
    if (newDirection !== direction) {
      setDirection(newDirection)
      setInputValue('')
      setOutputValue('')
      setLastEdited('input')
      setPreview(null)
      setPreviewError(null)
    }
  }

  const handleSlippageSelect = (value: number) => {
    setSlippage(value)
    setShowCustomSlippage(false)
    setCustomSlippage('')
  }

  const handleCustomSlippageChange = (value: string) => {
    setCustomSlippage(value)
    const parsed = parseFloat(value)
    if (!isNaN(parsed) && parsed > 0 && parsed < 50) {
      setSlippage(parsed)
    }
  }

  const handleSwap = async () => {
    if (!preview) return

    const amountRaw = getAmountRaw()
    if (!amountRaw) return

    setLoading(true)
    setError(null)

    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const buildResult = await buildDexySwapTx(
        variant,
        direction,
        amountRaw,
        preview.min_output,
        walletAddress,
        utxos as object[],
        nodeStatus.chain_height,
        recipientOrNull,
      )

      const inputLabel = direction === 'erg_to_dexy'
        ? `${inputValue} ERG`
        : `${inputValue} ${config.name}`
      const outputLabel = direction === 'erg_to_dexy'
        ? config.name
        : 'ERG'
      const message = `Swap ${inputLabel} for ${outputLabel}`

      const signResult = await startDexySwapSign(buildResult.unsigned_tx, message)

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const handlePreviewConfirm = () => {
    handleSwap()
  }

  const formatToken = (rawAmount: number, decimals: number) => {
    if (decimals === 0) {
      return rawAmount.toLocaleString(undefined, { maximumFractionDigits: 0 })
    }
    const divisor = Math.pow(10, decimals)
    return (rawAmount / divisor).toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: decimals,
    })
  }


  const formatMinOutput = () => {
    if (!preview) return '0'
    if (direction === 'erg_to_dexy') {
      return formatToken(preview.min_output, config.decimals)
    } else {
      return formatErg(preview.min_output)
    }
  }

  const outputTokenLabel = direction === 'erg_to_dexy' ? config.name : 'ERG'
  const inputTokenLabel = direction === 'erg_to_dexy' ? 'ERG' : config.name

  // Validation -- check that the active field has a valid value
  const activeValue = lastEdited === 'input' ? inputValue : outputValue
  const activeAmount = parseFloat(activeValue)
  const hasValidInput = !isNaN(activeAmount) && activeAmount > 0
  const inputAmount = parseFloat(inputValue)
  const hasEnoughBalance = direction === 'erg_to_dexy'
    ? !isNaN(inputAmount) && inputAmount > 0 && inputAmount <= (ergBalance / 1e9) - 0.0012
    : !isNaN(inputAmount) && inputAmount > 0 && inputAmount <= dexyBalance / Math.pow(10, config.decimals)
  const canSwap = hasValidInput && hasEnoughBalance && preview && !previewLoading && !previewError

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal dexy-swap-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Swap {config.name}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="swap-input-step">
              {/* Direction Toggle */}
              <div className="direction-toggle">
                <button
                  className={`direction-btn ${direction === 'erg_to_dexy' ? 'active' : ''}`}
                  onClick={() => handleDirectionChange('erg_to_dexy')}
                >
                  ERG &rarr; {config.name}
                </button>
                <button
                  className={`direction-btn ${direction === 'dexy_to_erg' ? 'active' : ''}`}
                  onClick={() => handleDirectionChange('dexy_to_erg')}
                >
                  {config.name} &rarr; ERG
                </button>
              </div>

              {/* Input Field */}
              <div className="swap-inputs">
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Pay</span>
                    <span className="swap-field-balance">
                      Balance: {direction === 'erg_to_dexy'
                        ? `${formatErg(ergBalance)} ERG`
                        : `${formatToken(dexyBalance, config.decimals)} ${config.name}`
                      }
                    </span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={inputValue}
                      onChange={e => { setInputValue(e.target.value); setLastEdited('input') }}
                      placeholder="0"
                      min="0"
                      step={direction === 'erg_to_dexy' ? '0.0001' : Math.pow(10, -config.decimals)}
                    />
                    <div className="swap-field-token">
                      <span>{inputTokenLabel}</span>
                      <button className="max-btn" onClick={handleMaxClick} type="button">
                        MAX
                      </button>
                    </div>
                  </div>
                </div>

                {/* Arrow separator */}
                <div className="swap-arrow">&darr;</div>

                {/* You Receive */}
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Receive</span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={outputValue}
                      onChange={e => { setOutputValue(e.target.value); setLastEdited('output') }}
                      placeholder="0"
                      min="0"
                      step={direction === 'erg_to_dexy' ? Math.pow(10, -config.decimals) : '0.0001'}
                    />
                    <span className="swap-field-token">{outputTokenLabel}</span>
                  </div>
                </div>
              </div>

              {/* Slippage Selector */}
              <div className="slippage-selector">
                <span className="slippage-label">Slippage Tolerance</span>
                <div className="slippage-options">
                  {SLIPPAGE_OPTIONS.map(opt => (
                    <button
                      key={opt}
                      className={`slippage-btn ${slippage === opt && !showCustomSlippage ? 'active' : ''}`}
                      onClick={() => handleSlippageSelect(opt)}
                    >
                      {opt}%
                    </button>
                  ))}
                  <button
                    className={`slippage-btn ${showCustomSlippage ? 'active' : ''}`}
                    onClick={() => setShowCustomSlippage(true)}
                  >
                    Custom
                  </button>
                </div>
                {showCustomSlippage && (
                  <div className="slippage-custom-input">
                    <input
                      type="number"
                      value={customSlippage}
                      onChange={e => handleCustomSlippageChange(e.target.value)}
                      placeholder="0.5"
                      min="0.01"
                      max="49"
                      step="0.1"
                    />
                    <span>%</span>
                  </div>
                )}
              </div>

              {/* Validation Warnings */}
              {hasValidInput && !hasEnoughBalance && (
                <div className="message warning">
                  Insufficient {inputTokenLabel} balance
                </div>
              )}

              {previewError && (
                <div className="message error">{previewError}</div>
              )}

              {/* Info Section */}
              <div className="mint-info">
                <div className="info-row">
                  <span>LP Fee</span>
                  <span>0.3%</span>
                </div>
                {state && (() => {
                  const oracleRate = (state.oracle_rate_nano / 1e9) * Math.pow(10, config.decimals)
                  const swapRate = lpRate * 1.003
                  const mintBetter = state.can_mint && oracleRate < swapRate
                  const savingPct = Math.abs(oracleRate - swapRate) / Math.max(oracleRate, swapRate) * 100
                  return savingPct > 0.1 ? (
                    <div className="info-row">
                      <span>vs Mint</span>
                      <span className={mintBetter ? 'negative' : 'positive'}>
                        {mintBetter
                          ? `Mint ${savingPct.toFixed(1)}% cheaper`
                          : `LP Swap ${savingPct.toFixed(1)}% cheaper`}
                      </span>
                    </div>
                  ) : null
                })()}
                {preview && (
                  <>
                    <div className="info-row">
                      <span>Price Impact</span>
                      <span className={preview.price_impact > 3 ? 'high-impact' : ''}>
                        {preview.price_impact.toFixed(2)}%
                      </span>
                    </div>
                    <div className="info-row">
                      <span>Minimum Received</span>
                      <span>{formatMinOutput()} {outputTokenLabel}</span>
                    </div>
                    <div className="info-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(preview.miner_fee_nano)} ERG</span>
                    </div>
                    {state && (
                      <div className="info-row">
                        <span>LP Reserves</span>
                        <span>
                          {formatErg(preview.lp_erg_reserves)} ERG / {formatToken(preview.lp_dexy_reserves, config.decimals)} {config.name}
                        </span>
                      </div>
                    )}
                  </>
                )}
              </div>

              {/* High price impact warning */}
              {preview && preview.price_impact > 5 && (
                <div className="message warning">
                  High price impact ({preview.price_impact.toFixed(2)}%). Consider a smaller swap.
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
                  onClick={handlePreviewConfirm}
                  disabled={loading || !canSwap || (!!recipientAddress && addressValid !== true)}
                >
                  {loading ? 'Building...' : 'Swap'}
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
              <h3>Swap Submitted!</h3>
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
              <h3>Swap Failed</h3>
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
