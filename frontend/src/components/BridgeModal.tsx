import { useState } from 'react'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildBridgeLockTx,
  startBridgeSign,
  getBridgeTxStatus,
  chainDisplayName,
  formatTokenAmount,
  type BridgeTokenInfo,
  type BridgeFeeInfo,
} from '../api/rosen'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'

interface BridgeModalProps {
  isOpen: boolean
  onClose: () => void
  token: BridgeTokenInfo
  amount: string
  targetChain: string
  targetAddress: string
  fees: BridgeFeeInfo
  walletAddress: string
  explorerUrl: string
  onSuccess: () => void
}

type Step = 'confirm' | 'building' | 'signing' | 'success' | 'error'

export function BridgeModal({
  isOpen,
  onClose,
  token,
  amount,
  targetChain,
  targetAddress,
  fees,
  walletAddress: _walletAddress,
  explorerUrl,
  onSuccess,
}: BridgeModalProps) {
  void _walletAddress
  const [step, setStep] = useState<Step>('confirm')
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: getBridgeTxStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'Rosen', operation: 'bridge', description: `Bridge ${amount} ${token.name}` },
  })

  const baseAmount = Math.floor(parseFloat(amount) * Math.pow(10, token.decimals))

  const handleConfirm = async () => {
    setStep('building')
    setError(null)

    try {
      const result = await buildBridgeLockTx(
        token.ergoTokenId,
        baseAmount,
        targetChain,
        targetAddress,
        fees.bridgeFeeRaw,
        fees.networkFeeRaw,
      )

      const message = `Bridge ${amount} ${token.name} to ${chainDisplayName(targetChain)}`
      const signResult = await startBridgeSign(result.unsignedTx, message)

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal bridge-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Bridge to {chainDisplayName(targetChain)}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {/* Confirm Step */}
          {step === 'confirm' && (
            <div className="bridge-confirm-step">
              <div className="bridge-preview-section">
                <div className="bridge-preview-row">
                  <span>Token</span>
                  <span>{token.name}</span>
                </div>
                <div className="bridge-preview-row">
                  <span>Amount</span>
                  <span>{amount} {token.name}</span>
                </div>
                <div className="bridge-preview-row">
                  <span>Destination</span>
                  <span>{chainDisplayName(targetChain)}</span>
                </div>
                <div className="bridge-preview-row address">
                  <span>To Address</span>
                  <span className="bridge-address-display" title={targetAddress}>
                    {targetAddress.slice(0, 16)}...{targetAddress.slice(-8)}
                  </span>
                </div>

                <div className="bridge-preview-divider" />

                <div className="bridge-preview-row fee">
                  <span>Bridge Fee</span>
                  <span>{formatTokenAmount(parseInt(fees.bridgeFee), token.decimals)} {token.name}</span>
                </div>
                <div className="bridge-preview-row fee">
                  <span>Network Fee</span>
                  <span>{formatTokenAmount(parseInt(fees.networkFee), token.decimals)} {token.name}</span>
                </div>
                {fees.feeRatioBps > 0 && (
                  <div className="bridge-preview-row fee">
                    <span>Variable Fee ({(fees.feeRatioBps / 100).toFixed(2)}%)</span>
                    <span>
                      {formatTokenAmount(
                        Math.floor(baseAmount * fees.feeRatioBps / 10000),
                        token.decimals
                      )} {token.name}
                    </span>
                  </div>
                )}

                <div className="bridge-preview-divider" />

                <div className="bridge-preview-row highlight">
                  <span>You Receive</span>
                  <span>{formatTokenAmount(parseInt(fees.receivingAmount), token.decimals)} {token.name}</span>
                </div>
              </div>

              <button className="bridge-submit-btn" onClick={handleConfirm}>
                Confirm Bridge
              </button>
            </div>
          )}

          {/* Building Step */}
          {step === 'building' && (
            <div className="bridge-centered">
              <div className="spinner-small" />
              <p>Building lock transaction...</p>
            </div>
          )}

          {/* Signing Step */}
          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="mint-signing-step">
              <p>Choose how to sign the transaction:</p>
              <div className="wallet-options">
                <button className="wallet-option" onClick={flow.handleNautilusSign}>
                  <div className="wallet-option-icon">
                    <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" strokeWidth="1.5">
                      <rect x="2" y="4" width="20" height="16" rx="2" />
                      <path d="M22 10H18a2 2 0 0 0 0 4h4" />
                    </svg>
                  </div>
                  <span>Nautilus</span>
                  <span className="wallet-option-hint">Browser Extension</span>
                </button>
                <button className="wallet-option" onClick={flow.handleMobileSign}>
                  <div className="wallet-option-icon">
                    <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" strokeWidth="1.5">
                      <rect x="5" y="2" width="14" height="20" rx="2" />
                      <line x1="12" y1="18" x2="12" y2="18" strokeLinecap="round" />
                    </svg>
                  </div>
                  <span>Mobile Wallet</span>
                  <span className="wallet-option-hint">Scan QR Code</span>
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div className="nautilus-waiting">
              <div className="nautilus-icon">
                <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <rect x="2" y="4" width="20" height="16" rx="2" />
                  <path d="M22 10H18a2 2 0 0 0 0 4h4" />
                </svg>
              </div>
              <p>Approve the transaction in Nautilus</p>
              <div className="spinner-small" />
              <div className="button-group">
                <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                  Back
                </button>
                <button className="btn btn-secondary" onClick={flow.handleNautilusSign}>
                  Open Nautilus Again
                </button>
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
              <div className="button-group">
                <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                  Back
                </button>
              </div>
            </div>
          )}

          {/* Success Step */}
          {step === 'success' && (
            <div className="success-step">
              <div className="success-icon">
                <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Bridge Transaction Submitted!</h3>
              <p>Your {token.name} will be bridged to {chainDisplayName(targetChain)}.</p>
              <p className="bridge-note">The bridge watchers will process your transfer. This typically takes 10-30 minutes.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { onSuccess(); onClose() }}>
                Done
              </button>
            </div>
          )}

          {/* Error Step */}
          {step === 'error' && (
            <div className="error-step">
              <div className="error-icon">
                <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--red-400)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="15" y1="9" x2="9" y2="15" />
                  <line x1="9" y1="9" x2="15" y2="15" />
                </svg>
              </div>
              <h3>Bridge Failed</h3>
              <p className="error-message">{error}</p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={onClose}>
                  Close
                </button>
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
