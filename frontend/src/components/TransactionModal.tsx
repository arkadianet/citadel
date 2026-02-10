import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { AdvancedOptions, useRecipientAddress } from './AdvancedOptions'

export type SigmaUsdAction = 'mint_sigusd' | 'redeem_sigusd' | 'mint_sigrsv' | 'redeem_sigrsv'

interface PreviewResponse {
  erg_amount_nano: string
  protocol_fee_nano: string
  tx_fee_nano: string
  total_erg_nano: string
  token_amount: string
  token_name: string
  can_execute: boolean
  error: string | null
}

interface TransactionModalProps {
  isOpen: boolean
  onClose: () => void
  action: SigmaUsdAction
  walletAddress: string
  ergBalance: number
  tokenBalance?: number // For redeem operations
  explorerUrl: string
  onSuccess: (txId: string) => void
}

type TxStep = 'input' | 'preview' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'

const ACTION_CONFIG = {
  mint_sigusd: {
    title: 'Mint SigUSD',
    inputLabel: 'Amount (SigUSD)',
    decimals: 2,
    isRedeem: false,
    tokenName: 'SigUSD',
  },
  redeem_sigusd: {
    title: 'Redeem SigUSD',
    inputLabel: 'Amount (SigUSD)',
    decimals: 2,
    isRedeem: true,
    tokenName: 'SigUSD',
  },
  mint_sigrsv: {
    title: 'Mint SigRSV',
    inputLabel: 'Amount (SigRSV)',
    decimals: 0,
    isRedeem: false,
    tokenName: 'SigRSV',
  },
  redeem_sigrsv: {
    title: 'Redeem SigRSV',
    inputLabel: 'Amount (SigRSV)',
    decimals: 0,
    isRedeem: true,
    tokenName: 'SigRSV',
  },
}

export function TransactionModal({
  isOpen,
  onClose,
  action,
  walletAddress,
  ergBalance,
  tokenBalance,
  explorerUrl,
  onSuccess,
}: TransactionModalProps) {
  const config = ACTION_CONFIG[action]
  const { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } = useRecipientAddress()
  const [step, setStep] = useState<TxStep>('input')
  const [amount, setAmount] = useState('')
  const [preview, setPreview] = useState<PreviewResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')

  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setAmount('')
      setPreview(null)
      setError(null)
      setQrUrl(null)
      setNautilusUrl(null)
      setRequestId(null)
      setTxId(null)
      setSignMethod('choose')
      setRecipientAddress('')
    }
  }, [isOpen, action])

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

  const handlePreview = async () => {
    const multiplier = config.decimals === 2 ? 100 : 1
    const amountRaw = Math.round(parseFloat(amount) * multiplier)
    if (isNaN(amountRaw) || amountRaw <= 0) {
      setError('Please enter a valid amount')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const result = await invoke<PreviewResponse>('preview_sigmausd_tx', {
        request: { action, amount: amountRaw, user_address: walletAddress }
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
    if (!preview) return

    setLoading(true)
    setError(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')

      const multiplier = config.decimals === 2 ? 100 : 1
      const amountRaw = Math.round(parseFloat(amount) * multiplier)

      const buildResult = await invoke<{ unsigned_tx: object; summary: object }>('build_sigmausd_tx', {
        request: {
          action,
          amount: amountRaw,
          user_address: walletAddress,
          user_utxos: utxos,
          current_height: nodeStatus.chain_height,
          recipient_address: recipientOrNull,
        }
      })

      const signResult = await invoke<{ request_id: string; ergopay_url: string; nautilus_url: string }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `${config.title}: ${amount} ${config.tokenName}`
        }
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
      <div className="modal mint-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{config.title}</h2>
          <button className="close-btn" onClick={onClose}>×</button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="mint-input-step">
              <div className="form-group">
                <label className="form-label">{config.inputLabel}</label>
                <input
                  type="number"
                  className="input"
                  value={amount}
                  onChange={e => setAmount(e.target.value)}
                  placeholder="0.00"
                  min={config.decimals === 2 ? "0.01" : "1"}
                  step={config.decimals === 2 ? "0.01" : "1"}
                />
              </div>
              <p className="balance-hint">
                {config.isRedeem
                  ? `Available: ${tokenBalance ?? 0} ${config.tokenName}`
                  : `Available: ${(ergBalance / 1e9).toFixed(4)} ERG`}
              </p>
              <AdvancedOptions
                recipientAddress={recipientAddress}
                onRecipientChange={setRecipientAddress}
                addressValid={addressValid}
              />
              {error && <div className="message error">{error}</div>}
              <button
                className="btn btn-primary"
                onClick={handlePreview}
                disabled={loading || !amount || (!!recipientAddress && addressValid !== true)}
              >
                {loading ? 'Calculating...' : 'Preview'}
              </button>
            </div>
          )}

          {step === 'preview' && preview && (
            <div className="mint-preview-step">
              <div className="preview-summary">
                <div className="preview-row">
                  <span>{config.isRedeem ? 'You Provide' : 'You Pay'}</span>
                  <span>
                    {config.isRedeem
                      ? `${amount} ${config.tokenName}`
                      : `${formatErg(Number(preview.total_erg_nano))} ERG`}
                  </span>
                </div>
                <div className="preview-row detail">
                  <span>{config.isRedeem ? 'ERG Value' : 'Base Cost'}</span>
                  <span>{formatErg(Number(preview.erg_amount_nano))} ERG</span>
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
                  <span>
                    {config.isRedeem
                      ? `${formatErg(Number(preview.erg_amount_nano))} ERG`
                      : `${amount} ${config.tokenName}`}
                  </span>
                </div>
              </div>
              {error && <div className="message error">{error}</div>}
              <div className="button-group">
                <button className="btn btn-secondary" onClick={() => setStep('input')}>Back</button>
                <button className="btn btn-primary" onClick={handleSign} disabled={loading}>
                  {loading ? 'Building...' : 'Sign with Wallet'}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && signMethod === 'choose' && (
            <div className="mint-signing-step">
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
            <div className="mint-success-step">
              <div className="success-icon">✓</div>
              <h3>Transaction Submitted!</h3>
              <p>Your {config.title.toLowerCase()} transaction has been submitted.</p>
              {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { if (txId) onSuccess(txId); onClose(); }}>Done</button>
            </div>
          )}

          {step === 'error' && (
            <div className="mint-error-step">
              <div className="error-icon">✕</div>
              <h3>Transaction Failed</h3>
              <p>{error}</p>
              <button className="btn btn-primary" onClick={() => setStep('input')}>Try Again</button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
