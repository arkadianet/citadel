import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildSwapRefundTx, startRefundSign, getRefundTxStatus,
  formatOrderInput, type PendingOrder,
} from '../api/orders'
import { formatErg } from '../api/amm'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import { TxSuccess } from './TxSuccess'

interface SwapRefundModalProps {
  isOpen: boolean
  onClose: () => void
  order: PendingOrder
  walletAddress: string
  explorerUrl: string
  onSuccess: () => void
}

type RefundStep = 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'

export function SwapRefundModal({
  isOpen,
  onClose,
  order,
  walletAddress: _walletAddress,
  explorerUrl,
  onSuccess,
}: SwapRefundModalProps) {
  void _walletAddress
  const { navigateToExplorer } = useExplorerNav()
  const [step, setStep] = useState<RefundStep>('confirm')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [refundSummary, setRefundSummary] = useState<{ refundedErg: number; minerFee: number } | null>(null)

  useEffect(() => {
    if (isOpen) {
      setStep('confirm')
      setError(null)
      setQrUrl(null)
      setNautilusUrl(null)
      setRequestId(null)
      setTxId(null)
      setSignMethod('choose')
      setRefundSummary(null)
    }
  }, [isOpen])

  // Poll for tx status during signing
  useEffect(() => {
    if (step !== 'signing' || !requestId) return

    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await getRefundTxStatus(requestId)
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

  const handleConfirmRefund = async () => {
    setLoading(true)
    setError(null)
    setStep('building')

    try {
      // Get user ErgoTree from UTXOs
      const utxos = await invoke<Array<{ ergo_tree?: string; ergoTree?: string }>>('get_user_utxos')
      if (!utxos || utxos.length === 0) {
        throw new Error('No user UTXOs available')
      }
      const userErgoTree = utxos[0].ergo_tree || utxos[0].ergoTree
      if (!userErgoTree) {
        throw new Error('Cannot determine user ErgoTree')
      }

      // Build refund tx — backend fetches proxy box by ID
      const buildResult = await buildSwapRefundTx(order.boxId, userErgoTree)

      setRefundSummary({
        refundedErg: buildResult.summary.input_amount,
        minerFee: buildResult.summary.miner_fee,
      })

      const message = `Refund swap order ${order.txId.slice(0, 8)}...`
      const signResult = await startRefundSign(buildResult.unsigned_tx, message)

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

  const handleMobileSign = () => {
    setSignMethod('mobile')
  }

  const handleBackToChoice = () => {
    setSignMethod('choose')
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal swap-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Refund Swap Order</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'confirm' && (
            <div className="swap-preview-step">
              <div className="preview-section">
                <div className="preview-row">
                  <span>Order TX</span>
                  <button
                    className="link-button"
                    onClick={() => navigateToExplorer({ page: 'transaction', id: order.txId })}
                  >
                    {order.txId.slice(0, 12)}...
                  </button>
                </div>
                <div className="preview-row highlight">
                  <span>Input</span>
                  <span>{formatOrderInput(order.input)}</span>
                </div>
                <div className="preview-row">
                  <span>Box Value</span>
                  <span>{formatErg(order.valueNanoErg)} ERG</span>
                </div>
                <div className="preview-row">
                  <span>Box ID</span>
                  <span style={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>{order.boxId.slice(0, 16)}...</span>
                </div>
              </div>

              <div className="warning-box">
                This will cancel your swap order and return all funds to your wallet.
                A miner fee of ~0.0011 ERG will be deducted.
              </div>

              <div className="button-group">
                <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
                <button
                  className="btn btn-primary"
                  onClick={handleConfirmRefund}
                  disabled={loading}
                  style={{ background: 'var(--red-500, #ef4444)' }}
                >
                  {loading ? 'Building...' : 'Confirm Refund'}
                </button>
              </div>
            </div>
          )}

          {step === 'building' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <span>Building refund transaction...</span>
            </div>
          )}

          {step === 'signing' && signMethod === 'choose' && (
            <div className="mint-signing-step">
              {refundSummary && (
                <div className="preview-section" style={{ marginBottom: 'var(--space-md)' }}>
                  <div className="preview-row highlight">
                    <span>You Receive</span>
                    <span className="text-emerald">{formatErg(refundSummary.refundedErg)} ERG</span>
                  </div>
                  <div className="preview-row">
                    <span>Miner Fee</span>
                    <span>{formatErg(refundSummary.minerFee)} ERG</span>
                  </div>
                </div>
              )}
              <p>Choose your signing method</p>
              <div className="wallet-options">
                <button className="wallet-option" onClick={handleNautilusSign}>
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

                <button className="wallet-option" onClick={handleMobileSign}>
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

          {step === 'signing' && signMethod === 'nautilus' && (
            <div className="mint-signing-step">
              <p>Approve the refund in Nautilus</p>
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
                <button className="btn btn-secondary" onClick={handleBackToChoice}>Back</button>
                <button className="btn btn-primary" onClick={handleNautilusSign}>Open Nautilus Again</button>
              </div>
            </div>
          )}

          {step === 'signing' && signMethod === 'mobile' && qrUrl && (
            <div className="mint-signing-step">
              <p>Scan with your Ergo wallet to sign</p>
              <div className="qr-container">
                <QRCodeSVG value={qrUrl} size={200} />
              </div>
              <p className="signing-hint">Waiting for signature...</p>
              <button className="btn btn-secondary" onClick={handleBackToChoice}>Back</button>
            </div>
          )}

          {step === 'success' && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>Refund Submitted!</h3>
              <p>Your funds will be returned to your wallet.</p>
              {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={onSuccess}>Done</button>
            </div>
          )}

          {step === 'error' && (
            <div className="error-step">
              <div className="error-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M15 9l-6 6M9 9l6 6" />
                </svg>
              </div>
              <h3>Refund Failed</h3>
              <p className="error-message">{error}</p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={onClose}>Close</button>
                <button className="btn btn-primary" onClick={() => { setStep('confirm'); setError(null) }}>
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
