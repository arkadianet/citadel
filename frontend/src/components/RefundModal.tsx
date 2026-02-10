/**
 * RefundModal Component
 *
 * Modal for recovering funds from stuck proxy boxes that weren't processed
 * by the Duckpools bots. Users can reclaim their funds after the refund height
 * stored in the proxy box has passed (~720 blocks / ~24 hours after creation).
 */

import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildRefundTx,
  type LendingBuildResponse,
} from '../api/lending'
import { TX_FEE_NANO } from '../constants'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import './LendModal.css' // Reuse LendModal styles

interface RefundModalProps {
  /** Whether the modal is open */
  isOpen: boolean
  /** Callback to close the modal */
  onClose: () => void
  /** User's wallet address (if connected) */
  userAddress: string | null
  /** Explorer URL for transaction links */
  explorerUrl: string
  /** Callback when transaction succeeds */
  onSuccess: () => void
}

type TxStep = 'input' | 'checking' | 'preview' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'

interface ProxyBoxInfo {
  box_id: string
  value_nano: number
  refund_height: number
  current_height: number
  can_refund: boolean
  blocks_until_refund: number
}

export function RefundModal({
  isOpen,
  onClose,
  userAddress,
  explorerUrl,
  onSuccess,
}: RefundModalProps) {
  // Step state
  const [step, setStep] = useState<TxStep>('input')
  const [proxyBoxId, setProxyBoxId] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Proxy box info (fetched when checking eligibility)
  const [proxyBoxInfo, setProxyBoxInfo] = useState<ProxyBoxInfo | null>(null)

  // Build response
  const [buildResponse, setBuildResponse] = useState<LendingBuildResponse | null>(null)

  // Signing state
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setProxyBoxId('')
      setLoading(false)
      setError(null)
      setProxyBoxInfo(null)
      setBuildResponse(null)
      setSignMethod('choose')
      setQrUrl(null)
      setNautilusUrl(null)
      setRequestId(null)
      setTxId(null)
    }
  }, [isOpen])

  // Poll for transaction status during signing
  useEffect(() => {
    if (step !== 'signing' || !requestId) return

    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await invoke<{ status: string; tx_id: string | null; error: string | null }>(
          'get_mint_tx_status',
          { requestId }
        )

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

  // Check if the proxy box is eligible for refund
  const handleCheckEligibility = useCallback(async () => {
    if (!proxyBoxId.trim()) {
      setError('Please enter a proxy box ID')
      return
    }

    if (!userAddress) {
      setError('Please connect your wallet first')
      return
    }

    // Validate box ID format (64 hex characters)
    const boxIdRegex = /^[a-fA-F0-9]{64}$/
    if (!boxIdRegex.test(proxyBoxId.trim())) {
      setError('Invalid box ID format. Box IDs are 64 hexadecimal characters.')
      return
    }

    setLoading(true)
    setError(null)
    setStep('checking')

    try {
      // Get current height and check if box exists
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const currentHeight = nodeStatus.chain_height

      // Try to get the box info via the node
      // The backend will validate if it's a valid proxy box and check refund eligibility
      const boxInfo = await invoke<{
        value_nano: number
        refund_height: number
        is_proxy_box: boolean
      }>('check_proxy_box', { boxId: proxyBoxId.trim() })

      if (!boxInfo.is_proxy_box) {
        setError('This box is not a valid Duckpools proxy box')
        setStep('input')
        setLoading(false)
        return
      }

      const canRefund = currentHeight >= boxInfo.refund_height
      const blocksUntilRefund = canRefund ? 0 : boxInfo.refund_height - currentHeight

      setProxyBoxInfo({
        box_id: proxyBoxId.trim(),
        value_nano: boxInfo.value_nano,
        refund_height: boxInfo.refund_height,
        current_height: currentHeight,
        can_refund: canRefund,
        blocks_until_refund: blocksUntilRefund,
      })

      if (canRefund) {
        // Automatically proceed to build the transaction
        await handleBuildRefund(proxyBoxId.trim(), currentHeight)
      } else {
        setStep('input')
        setError(`This box cannot be refunded yet. ${blocksUntilRefund} blocks remaining (~${Math.ceil(blocksUntilRefund * 2 / 60)} hours).`)
      }
    } catch (e) {
      const errorMsg = String(e)
      if (errorMsg.includes('not found') || errorMsg.includes('Box not found')) {
        setError('Box not found. It may have already been spent or processed.')
      } else {
        setError(errorMsg)
      }
      setStep('input')
    } finally {
      setLoading(false)
    }
  }, [proxyBoxId, userAddress])

  // Build the refund transaction
  const handleBuildRefund = useCallback(async (boxId: string, currentHeight: number) => {
    if (!userAddress) {
      setError('Please connect your wallet first')
      return
    }

    setLoading(true)
    setError(null)

    try {
      // Get user UTXOs - the proxy box should be included
      const utxos = await invoke<unknown[]>('get_user_utxos')

      // Build the refund transaction
      const response = await buildRefundTx({
        proxy_box_id: boxId,
        user_address: userAddress,
        user_utxos: utxos,
        current_height: currentHeight,
      })

      setBuildResponse(response)
      setStep('preview')
    } catch (e) {
      setError(String(e))
      setStep('input')
    } finally {
      setLoading(false)
    }
  }, [userAddress])

  // Start signing flow
  const handleSign = useCallback(async () => {
    if (!buildResponse) return

    setLoading(true)
    setError(null)

    try {
      const signResult = await invoke<{
        request_id: string
        ergopay_url: string
        nautilus_url: string
      }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResponse.unsigned_tx,
          message: 'Refund stuck proxy box transaction',
        },
      })

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
  }, [buildResponse])

  // Handle Nautilus signing
  const handleNautilusSign = useCallback(async () => {
    if (!nautilusUrl) return
    setSignMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      setError(String(e))
    }
  }, [nautilusUrl])

  // Handle mobile signing
  const handleMobileSign = useCallback(() => {
    setSignMethod('mobile')
  }, [])

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal lend-modal refund-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Recover Stuck Transaction</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="lend-input-step">
              {/* Explanation */}
              <div className="pool-info-card refund-explanation">
                <div className="refund-icon">
                  <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="var(--warning, #f59e0b)" strokeWidth="1.5">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M12 8v4" />
                    <path d="M12 16h.01" />
                  </svg>
                </div>
                <h3>What is a refund?</h3>
                <p>
                  If your lend, withdraw, or repay transaction was not processed by the Duckpools
                  bot (for example, due to network congestion or insufficient liquidity), your
                  funds are held in a &quot;proxy box&quot; on the blockchain.
                </p>
                <p>
                  After approximately 24 hours (~720 blocks), you can reclaim these funds by
                  building a refund transaction with the proxy box ID.
                </p>
              </div>

              {/* Wallet Connection Warning */}
              {!userAddress && (
                <div className="message warning">
                  Please connect your wallet to recover funds.
                </div>
              )}

              {/* Proxy Box ID Input */}
              <div className="form-group">
                <label className="form-label">Proxy Box ID</label>
                <div className="input-with-max">
                  <input
                    type="text"
                    className="input"
                    value={proxyBoxId}
                    onChange={(e) => setProxyBoxId(e.target.value)}
                    placeholder="Enter the 64-character box ID"
                    disabled={!userAddress}
                  />
                </div>
                <p className="balance-hint">
                  You can find the proxy box ID in your transaction history or on the Ergo explorer.
                </p>
              </div>

              {/* Eligibility Info */}
              {proxyBoxInfo && !proxyBoxInfo.can_refund && (
                <div className="pool-info-card">
                  <div className="pool-info-row">
                    <span className="pool-info-label">Box Value</span>
                    <span className="pool-info-value">{formatErg(proxyBoxInfo.value_nano)} ERG</span>
                  </div>
                  <div className="pool-info-row">
                    <span className="pool-info-label">Current Block</span>
                    <span className="pool-info-value">{proxyBoxInfo.current_height.toLocaleString()}</span>
                  </div>
                  <div className="pool-info-row">
                    <span className="pool-info-label">Refund Available At</span>
                    <span className="pool-info-value">{proxyBoxInfo.refund_height.toLocaleString()}</span>
                  </div>
                  <div className="pool-info-row">
                    <span className="pool-info-label">Blocks Remaining</span>
                    <span className="pool-info-value">{proxyBoxInfo.blocks_until_refund.toLocaleString()}</span>
                  </div>
                </div>
              )}

              {error && <div className="message error">{error}</div>}

              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleCheckEligibility}
                  disabled={loading || !userAddress || !proxyBoxId.trim()}
                >
                  {loading ? 'Checking...' : 'Check Eligibility'}
                </button>
              </div>
            </div>
          )}

          {step === 'checking' && (
            <div className="lend-signing-step">
              <div className="waiting-spinner" />
              <p>Checking proxy box eligibility...</p>
            </div>
          )}

          {step === 'preview' && buildResponse && proxyBoxInfo && (
            <div className="lend-preview-step">
              <div className="preview-summary">
                <div className="preview-header">
                  <span className="preview-label">You Will Recover</span>
                  <span className="preview-value">
                    {formatErg(proxyBoxInfo.value_nano - TX_FEE_NANO)} ERG
                  </span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>Proxy Box Value</span>
                    <span>{formatErg(proxyBoxInfo.value_nano)} ERG</span>
                  </div>
                  <div className="detail-row">
                    <span>Transaction Fee</span>
                    <span>-{formatErg(TX_FEE_NANO)} ERG</span>
                  </div>
                  <div className="detail-row total">
                    <span>Net Recovery</span>
                    <span>{formatErg(proxyBoxInfo.value_nano - TX_FEE_NANO)} ERG</span>
                  </div>
                </div>

                <p className="preview-note">
                  This transaction will recover your funds from the stuck proxy box and send
                  them back to your wallet.
                </p>
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
                  {loading ? 'Preparing...' : 'Sign Transaction'}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && (
            <div className="lend-signing-step">
              {signMethod === 'choose' && (
                <div className="sign-method-choice">
                  <p>Choose signing method:</p>
                  <div className="sign-methods">
                    <button className="sign-method-btn" onClick={handleNautilusSign}>
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="2" y="3" width="20" height="14" rx="2" />
                        <path d="M8 21h8" />
                        <path d="M12 17v4" />
                      </svg>
                      <span>Nautilus</span>
                      <small>Browser Extension</small>
                    </button>
                    <button className="sign-method-btn" onClick={handleMobileSign}>
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

              {signMethod === 'nautilus' && (
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
                  <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>
                    Back
                  </button>
                </div>
              )}

              {signMethod === 'mobile' && qrUrl && (
                <div className="qr-signing">
                  <p>Scan with Ergo Mobile Wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG
                      value={qrUrl}
                      size={200}
                      level="M"
                      includeMargin
                      bgColor="white"
                      fgColor="black"
                    />
                  </div>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>
                    Back
                  </button>
                </div>
              )}
            </div>
          )}

          {step === 'success' && (
            <div className="lend-success-step">
              <div className="success-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--success)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Funds Recovered!</h3>
              <p>Your refund transaction has been submitted to the network.</p>
              <p className="success-note">
                Your funds will be available in your wallet once the transaction is confirmed.
              </p>
              {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={onSuccess}>
                Done
              </button>
            </div>
          )}

          {step === 'error' && (
            <div className="lend-error-step">
              <div className="error-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--error)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="15" y1="9" x2="9" y2="15" />
                  <line x1="9" y1="9" x2="15" y2="15" />
                </svg>
              </div>
              <h3>Refund Failed</h3>
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

export default RefundModal
