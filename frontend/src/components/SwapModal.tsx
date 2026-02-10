import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  previewSwap, buildSwapTx, startSwapSign, getSwapTxStatus,
  previewDirectSwap, buildDirectSwapTx,
  getPoolDisplayName, formatTokenAmount, formatErg,
  type AmmPool, type SwapQuote, type SwapPreviewResponse, type DirectSwapPreviewResponse,
} from '../api/amm'
import { TxSuccess } from './TxSuccess'
import { AdvancedOptions, useRecipientAddress } from './AdvancedOptions'
import { useTransactionFlow } from '../hooks/useTransactionFlow'

interface SwapModalProps {
  isOpen: boolean
  onClose: () => void
  pool: AmmPool
  quote: SwapQuote
  inputAmount: string
  inputSide: 'x' | 'y'
  slippage: number
  nitro: number
  swapMode: 'proxy' | 'direct'
  walletAddress: string
  explorerUrl: string
  onSuccess: () => void
}

type SwapStep = 'preview' | 'signing' | 'success' | 'error'

function getInputType(pool: AmmPool, side: 'x' | 'y'): 'erg' | 'token' {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 'erg' : 'token'
  }
  return 'token'
}

function getInputTokenId(pool: AmmPool, side: 'x' | 'y'): string | undefined {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? undefined : pool.token_y.token_id
  }
  return side === 'x' ? pool.token_x?.token_id : pool.token_y.token_id
}

function getInputLabel(pool: AmmPool, side: 'x' | 'y'): string {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 'ERG' : (pool.token_y.name || pool.token_y.token_id.slice(0, 8))
  }
  if (side === 'x') {
    return pool.token_x?.name || pool.token_x?.token_id.slice(0, 8) || 'Token X'
  }
  return pool.token_y.name || pool.token_y.token_id.slice(0, 8)
}

function getInputDecimals(pool: AmmPool, side: 'x' | 'y'): number {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 9 : (pool.token_y.decimals ?? 0)
  }
  if (side === 'x') {
    return pool.token_x?.decimals ?? 0
  }
  return pool.token_y.decimals ?? 0
}

/** Unified preview type for both proxy and direct swaps */
type UnifiedPreview = SwapPreviewResponse | DirectSwapPreviewResponse

export function SwapModal({
  isOpen,
  onClose,
  pool,
  quote: _quote,
  inputAmount,
  inputSide,
  slippage,
  nitro,
  swapMode,
  walletAddress,
  explorerUrl,
  onSuccess,
}: SwapModalProps) {
  // _quote is available for future use; we fetch a fresh preview via previewSwap()
  void _quote
  const { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } = useRecipientAddress()
  const [step, setStep] = useState<SwapStep>('preview')
  const [preview, setPreview] = useState<UnifiedPreview | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: getSwapTxStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'AMM', operation: 'swap', description: `Swap ${getPoolDisplayName(pool)}` },
  })

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('preview')
      setPreview(null)
      setError(null)
      setRecipientAddress('')
      fetchPreview()
    }
  }, [isOpen])

  const fetchPreview = async () => {
    setLoading(true)
    setError(null)
    try {
      const inputType = getInputType(pool, inputSide)
      const tokenId = getInputTokenId(pool, inputSide)
      const decimals = getInputDecimals(pool, inputSide)
      const rawAmount = Math.round(parseFloat(inputAmount) * Math.pow(10, decimals))

      if (swapMode === 'direct') {
        const result = await previewDirectSwap(pool.pool_id, inputType, rawAmount, tokenId, slippage)
        setPreview(result)
      } else {
        const result = await previewSwap(pool.pool_id, inputType, rawAmount, tokenId, slippage, nitro)
        setPreview(result)
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const handleConfirmSwap = async () => {
    if (!preview) return
    setLoading(true)
    setError(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')

      const inputType = getInputType(pool, inputSide)
      const tokenId = getInputTokenId(pool, inputSide)
      const decimals = getInputDecimals(pool, inputSide)
      const rawAmount = Math.round(parseFloat(inputAmount) * Math.pow(10, decimals))

      let unsignedTx: object

      if (swapMode === 'direct') {
        const buildResult = await buildDirectSwapTx(
          pool.pool_id,
          inputType,
          rawAmount,
          tokenId,
          preview.min_output,
          walletAddress,
          utxos,
          nodeStatus.chain_height,
          recipientOrNull,
        )
        unsignedTx = buildResult.unsigned_tx
      } else {
        const executionFeeNano = Math.round(2_000_000 * nitro)
        const buildResult = await buildSwapTx(
          pool.pool_id,
          inputType,
          rawAmount,
          tokenId,
          preview.min_output,
          walletAddress,
          utxos,
          nodeStatus.chain_height,
          executionFeeNano,
          recipientOrNull,
        )
        unsignedTx = buildResult.unsigned_tx
      }

      const inputLabel = getInputLabel(pool, inputSide)
      const outputLabel = getInputLabel(pool, inputSide === 'x' ? 'y' : 'x')
      const modeLabel = swapMode === 'direct' ? 'Direct swap' : 'Swap'
      const message = `${modeLabel} ${inputAmount} ${inputLabel} for ${outputLabel} on ${getPoolDisplayName(pool)}`

      const signResult = await startSwapSign(unsignedTx, message)

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      const errMsg = String(e)
      if (swapMode === 'direct' && (errMsg.includes('not found') || errMsg.includes('double spending'))) {
        setError('Pool state changed since quote. Please try again.')
      } else {
        setError(errMsg)
      }
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  if (!isOpen) return null

  const inputLabel = getInputLabel(pool, inputSide)
  const outputLabel = getInputLabel(pool, inputSide === 'x' ? 'y' : 'x')

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal swap-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{swapMode === 'direct' ? 'Direct Swap' : 'Swap'} {inputLabel} &rarr; {outputLabel}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {/* Preview Step */}
          {step === 'preview' && (
            <div className="swap-preview-step">
              {loading && !preview && (
                <div className="swap-preview-loading">
                  <div className="spinner-small" />
                  <span>Fetching swap preview...</span>
                </div>
              )}

              {error && !preview && (
                <div className="swap-preview-error">
                  <div className="message error">{error}</div>
                  <button className="btn btn-secondary" onClick={fetchPreview}>Retry</button>
                </div>
              )}

              {preview && (
                <>
                  {/* Swap Summary */}
                  <div className="preview-section">
                    <div className="preview-row highlight">
                      <span>You Pay</span>
                      <span>{inputAmount} {inputLabel}</span>
                    </div>
                    <div className="preview-row highlight">
                      <span>You Receive (est.)</span>
                      <span className="text-emerald">
                        {formatTokenAmount(preview.output_amount, preview.output_decimals ?? 0)} {preview.output_token_name || outputLabel}
                      </span>
                    </div>
                    <div className="preview-row">
                      <span>Minimum Output</span>
                      <span>{formatTokenAmount(preview.min_output, preview.output_decimals ?? 0)} {preview.output_token_name || outputLabel}</span>
                    </div>
                  </div>

                  {/* Fee Breakdown */}
                  <div className="fee-breakdown">
                    <h4>Fee Breakdown</h4>
                    <div className="fee-row">
                      <span>Price Impact</span>
                      <span className={preview.price_impact > 3 ? 'text-danger' : preview.price_impact > 1 ? 'text-warning' : ''}>
                        {preview.price_impact.toFixed(2)}%
                      </span>
                    </div>
                    <div className="fee-row">
                      <span>Pool Fee</span>
                      <span>{preview.fee_amount.toLocaleString()}</span>
                    </div>
                    <div className="fee-row">
                      <span>Effective Rate</span>
                      <span>{preview.effective_rate.toFixed(6)}</span>
                    </div>
                    {'execution_fee_nano' in preview && (
                      <div className="fee-row">
                        <span>Execution Fee</span>
                        <span>{formatErg((preview as SwapPreviewResponse).execution_fee_nano)} ERG</span>
                      </div>
                    )}
                    <div className="fee-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(preview.miner_fee_nano)} ERG</span>
                    </div>
                    <div className="fee-row total">
                      <span>Total ERG Cost</span>
                      <span>{formatErg(preview.total_erg_cost_nano)} ERG</span>
                    </div>
                  </div>

                  {/* High Impact Warning */}
                  {preview.price_impact > 3 && (
                    <div className="warning-box">
                      Price impact is high ({preview.price_impact.toFixed(2)}%). You may want to reduce your trade size.
                    </div>
                  )}

                  {/* Slippage Notice */}
                  <div className="slippage-notice">
                    Slippage tolerance: {slippage}%
                  </div>

                  <AdvancedOptions
                    recipientAddress={recipientAddress}
                    onRecipientChange={setRecipientAddress}
                    addressValid={addressValid}
                  />

                  {error && <div className="message error">{error}</div>}

                  <div className="button-group">
                    <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
                    <button
                      className="btn btn-primary"
                      onClick={handleConfirmSwap}
                      disabled={loading || (!!recipientAddress && addressValid !== true)}
                    >
                      {loading ? 'Building...' : 'Confirm Swap'}
                    </button>
                  </div>
                </>
              )}
            </div>
          )}

          {/* Signing Step - Choose Method */}
          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="mint-signing-step">
              <p>Choose your signing method</p>
              <div className="wallet-options">
                <button className="wallet-option" onClick={flow.handleNautilusSign}>
                  <div className="wallet-option-icon">
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" />
                      <path d="M12 17v4" />
                    </svg>
                  </div>
                  <div className="wallet-option-info">
                    <span className="wallet-option-name">Nautilus Extension</span>
                    <span className="wallet-option-desc">Sign with browser extension</span>
                  </div>
                </button>

                <button className="wallet-option" onClick={flow.handleMobileSign}>
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
          )}

          {/* Signing Step - Nautilus */}
          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div className="mint-signing-step">
              <p>Approve the transaction in Nautilus</p>
              <div className="nautilus-waiting">
                <div className="nautilus-icon">
                  <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="1.5">
                    <rect x="2" y="3" width="20" height="14" rx="2" />
                    <path d="M8 21h8" />
                    <path d="M12 17v4" />
                  </svg>
                </div>
                <p className="signing-hint">Waiting for Nautilus approval...</p>
              </div>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                <button className="btn btn-primary" onClick={flow.handleNautilusSign}>Open Nautilus Again</button>
              </div>
            </div>
          )}

          {/* Signing Step - Mobile QR */}
          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div className="mint-signing-step">
              <p>Scan with your Ergo wallet to sign</p>
              <div className="qr-container">
                <QRCodeSVG value={flow.qrUrl} size={200} />
              </div>
              <p className="signing-hint">Waiting for signature...</p>
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
            </div>
          )}

          {/* Success Step */}
          {step === 'success' && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>Swap Submitted!</h3>
              <p>Your swap transaction has been submitted to the network.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { onSuccess() }}>Done</button>
            </div>
          )}

          {/* Error Step */}
          {step === 'error' && (
            <div className="error-step">
              <div className="error-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M15 9l-6 6M9 9l6 6" />
                </svg>
              </div>
              <h3>{swapMode === 'direct' ? 'Direct Swap Failed' : 'Swap Failed'}</h3>
              <p className="error-message">{error}</p>
              {swapMode === 'direct' && (
                <p style={{ fontSize: '0.85rem', color: 'var(--text-muted)', marginTop: 4 }}>
                  The pool state may have changed. Try again with a fresh quote.
                </p>
              )}
              <div className="button-group">
                <button className="btn btn-secondary" onClick={onClose}>Close</button>
                <button className="btn btn-primary" onClick={() => { setStep('preview'); setError(null); fetchPreview() }}>
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
