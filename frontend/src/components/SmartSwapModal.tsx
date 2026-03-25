import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  previewDirectSwap, buildDirectSwapTx, startSwapSign, getSwapTxStatus,
  formatErg, type DirectSwapPreviewResponse,
} from '../api/amm'
import type { RouteQuote } from '../api/router'
import { formatTokenAmount } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'

// =============================================================================
// Props
// =============================================================================

interface SmartSwapModalProps {
  isOpen: boolean
  onClose: () => void
  routeQuote: RouteQuote
  sourceAmount: number    // raw units
  slippage: number
  walletAddress: string
  explorerUrl: string
  onSuccess: () => void
}

type SwapStep = 'preview' | 'signing' | 'success' | 'error'

// =============================================================================
// SmartSwapModal
// =============================================================================

export function SmartSwapModal({
  isOpen,
  onClose,
  routeQuote,
  sourceAmount,
  slippage,
  walletAddress,
  explorerUrl,
  onSuccess,
}: SmartSwapModalProps) {
  const [step, setStep] = useState<SwapStep>('preview')
  const [preview, setPreview] = useState<DirectSwapPreviewResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Derive hop info from route
  const hop = routeQuote.route.hops[0]
  const inputType: 'erg' | 'token' = hop.token_in === 'ERG' ? 'erg' : 'token'
  const tokenId: string | undefined = inputType === 'token' ? hop.token_in : undefined
  const poolId = hop.pool_id

  const inputLabel = hop.token_in_name ?? (inputType === 'erg' ? 'ERG' : hop.token_in.slice(0, 8))
  const outputLabel = hop.token_out_name ?? hop.token_out.slice(0, 8)

  const flow = useTransactionFlow({
    pollStatus: getSwapTxStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: {
      protocol: 'AMM',
      operation: 'swap',
      description: `Direct swap ${inputLabel} → ${outputLabel}`,
    },
  })

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('preview')
      setPreview(null)
      setError(null)
      fetchPreview()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen])

  const fetchPreview = async () => {
    setLoading(true)
    setError(null)
    try {
      const result = await previewDirectSwap(poolId, inputType, sourceAmount, tokenId, slippage)
      setPreview(result)
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

      const buildResult = await buildDirectSwapTx(
        poolId,
        inputType,
        sourceAmount,
        tokenId,
        preview.min_output,
        walletAddress,
        utxos,
        nodeStatus.chain_height,
      )

      const message = `Direct swap ${inputLabel} → ${outputLabel}`
      const signResult = await startSwapSign(buildResult.unsigned_tx, message)

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      const errMsg = String(e)
      if (errMsg.includes('not found') || errMsg.includes('double spending')) {
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

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal smart-swap-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Direct Swap {inputLabel} &rarr; {outputLabel}</h2>
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
                      <span>{formatTokenAmount(sourceAmount, hop.token_in_decimals)} {inputLabel}</span>
                    </div>
                    <div className="preview-row highlight">
                      <span>You Receive (est.)</span>
                      <span className="text-emerald">
                        {formatTokenAmount(preview.output_amount, preview.output_decimals ?? 0)} {preview.output_token_name ?? outputLabel}
                      </span>
                    </div>
                    <div className="preview-row">
                      <span>Minimum Output</span>
                      <span>{formatTokenAmount(preview.min_output, preview.output_decimals ?? 0)} {preview.output_token_name ?? outputLabel}</span>
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

                  {error && <div className="message error">{error}</div>}

                  <div className="button-group">
                    <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
                    <button
                      className="btn btn-primary"
                      onClick={handleConfirmSwap}
                      disabled={loading}
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
              <h3>Direct Swap Failed</h3>
              <p className="error-message">{error}</p>
              <p style={{ fontSize: '0.85rem', color: 'var(--text-muted)', marginTop: 4 }}>
                The pool state may have changed. Try again with a fresh quote.
              </p>
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
