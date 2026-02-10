import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'

interface MintPreviewResponse {
  erg_cost_nano: string
  protocol_fee_nano: string
  tx_fee_nano: string
  total_cost_nano: string
  can_execute: boolean
  error: string | null
}

interface MintModalProps {
  isOpen: boolean
  onClose: () => void
  walletAddress: string
  ergBalance: number
  explorerUrl: string
  onSuccess: (txId: string) => void
}

type MintStep = 'input' | 'preview' | 'signing' | 'success' | 'error'

function pollMintStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}

export function MintModal({ isOpen, onClose, walletAddress, ergBalance, explorerUrl, onSuccess }: MintModalProps) {
  const [step, setStep] = useState<MintStep>('input')
  const [amount, setAmount] = useState('')
  const [preview, setPreview] = useState<MintPreviewResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: pollMintStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'SigmaUSD', operation: 'mint', description: 'SigmaUSD mint' },
  })

  // Reset modal-specific state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setAmount('')
      setPreview(null)
      setError(null)
    }
  }, [isOpen])

  const handlePreview = useCallback(async () => {
    const amountRaw = Math.round(parseFloat(amount) * 100) // Convert to raw units (2 decimals)
    if (isNaN(amountRaw) || amountRaw <= 0) {
      setError('Please enter a valid amount')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const result = await invoke<MintPreviewResponse>('preview_mint_sigusd', {
        request: { amount: amountRaw, user_address: walletAddress }
      })

      setPreview(result)
      if (result.can_execute) {
        setStep('preview')
      } else {
        setError(result.error || 'Cannot execute mint')
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [amount, walletAddress])

  const handleSign = useCallback(async () => {
    if (!preview) return

    setLoading(true)
    setError(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')
      const amountRaw = Math.round(parseFloat(amount) * 100)

      const buildResult = await invoke<{ unsigned_tx: object; summary: object }>('build_mint_sigusd', {
        request: {
          amount: amountRaw,
          user_address: walletAddress,
          user_utxos: utxos,
          current_height: nodeStatus.chain_height
        }
      })

      const signResult = await invoke<{ request_id: string; ergopay_url: string; nautilus_url: string }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `Mint ${amount} SigUSD`
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
  }, [preview, amount, walletAddress, flow])

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal mint-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Mint SigUSD</h2>
          <button className="close-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="mint-input-step">
              <div className="form-group">
                <label className="form-label">Amount (SigUSD)</label>
                <input
                  type="number"
                  className="input"
                  value={amount}
                  onChange={e => setAmount(e.target.value)}
                  placeholder="0.00"
                  min="0.01"
                  step="0.01"
                />
              </div>
              <p className="balance-hint">
                Available: {(ergBalance / 1e9).toFixed(4)} ERG
              </p>
              {error && <div className="message error">{error}</div>}
              <button
                className="btn btn-primary"
                onClick={handlePreview}
                disabled={loading || !amount}
              >
                {loading ? 'Calculating...' : 'Preview'}
              </button>
            </div>
          )}

          {step === 'preview' && preview && (
            <div className="mint-preview-step">
              <div className="preview-summary">
                <div className="preview-row">
                  <span>You Pay</span>
                  <span>{formatErg(Number(preview.total_cost_nano))} ERG</span>
                </div>
                <div className="preview-row detail">
                  <span>Base Cost</span>
                  <span>{formatErg(Number(preview.erg_cost_nano))} ERG</span>
                </div>
                <div className="preview-row detail">
                  <span>Protocol Fee (2%)</span>
                  <span>{formatErg(Number(preview.protocol_fee_nano))} ERG</span>
                </div>
                <div className="preview-row detail">
                  <span>Network Fee</span>
                  <span>{formatErg(Number(preview.tx_fee_nano))} ERG</span>
                </div>
                <div className="preview-row highlight">
                  <span>You Receive</span>
                  <span>{amount} SigUSD</span>
                </div>
              </div>
              {error && <div className="message error">{error}</div>}
              <div className="button-group">
                <button className="btn btn-secondary" onClick={() => setStep('input')}>
                  Back
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleSign}
                  disabled={loading}
                >
                  {loading ? 'Building...' : 'Sign with Wallet'}
                </button>
              </div>
            </div>
          )}

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

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div className="mint-signing-step">
              <p>Approve the transaction in Nautilus</p>
              <div className="nautilus-waiting">
                <div className="nautilus-icon">
                  <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="1.5">
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

          {step === 'success' && (
            <div className="mint-success-step">
              <div className="success-icon">✓</div>
              <h3>Transaction Submitted!</h3>
              <p>Your mint transaction has been submitted.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { if (flow.txId) onSuccess(flow.txId); onClose(); }}>
                Done
              </button>
            </div>
          )}

          {step === 'error' && (
            <div className="mint-error-step">
              <div className="error-icon">✕</div>
              <h3>Transaction Failed</h3>
              <p>{error}</p>
              <button className="btn btn-primary" onClick={() => setStep('input')}>
                Try Again
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
