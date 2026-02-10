import { useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  type HodlBankState,
  type HodlMintPreview,
  type HodlBurnPreview,
  previewHodlCoinMint,
  previewHodlCoinBurn,
  buildHodlCoinMintTx,
  buildHodlCoinBurnTx,
  startHodlCoinSign,
  getHodlCoinTxStatus,
  formatNanoErg,
} from '../api/hodlcoin'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import './HodlCoinModal.css'

interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

interface HodlCoinModalProps {
  isOpen: boolean
  onClose: () => void
  bank: HodlBankState
  walletAddress: string
  walletBalance: WalletBalance
  explorerUrl: string
  onSuccess: () => void
}

type Tab = 'mint' | 'burn'
type Step = 'input' | 'preview' | 'building' | 'signing' | 'success' | 'error'

export function HodlCoinModal({
  isOpen,
  onClose,
  bank,
  walletAddress: _walletAddress,
  walletBalance,
  explorerUrl,
  onSuccess,
}: HodlCoinModalProps) {
  void _walletAddress
  const [tab, setTab] = useState<Tab>('mint')
  const [step, setStep] = useState<Step>('input')

  // Mint state
  const [mintAmountStr, setMintAmountStr] = useState('')
  const [mintPreview, setMintPreview] = useState<HodlMintPreview | null>(null)

  // Burn state
  const [burnAmountStr, setBurnAmountStr] = useState('')
  const [burnPreview, setBurnPreview] = useState<HodlBurnPreview | null>(null)

  // Modal-specific state
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  const flow = useTransactionFlow({
    pollStatus: getHodlCoinTxStatus,
    isOpen,
    onSuccess: () => { setStep('success'); onSuccess() },
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'HodlCoin', operation: tab, description: `HodlCoin ${tab}` },
  })

  const bankName = bank.hodlTokenName || `hodl...${bank.hodlTokenId.slice(-6)}`
  const ergBalance = walletBalance.erg_nano
  const hodlToken = walletBalance.tokens.find(t => t.token_id === bank.hodlTokenId)
  const hodlBalanceRaw = hodlToken?.amount ?? 0
  const hodlDecimals = hodlToken?.decimals ?? 0
  const hodlDivisor = Math.pow(10, hodlDecimals)
  const hodlBalanceDisplay = hodlBalanceRaw / hodlDivisor

  // Parse input amounts (ERG for mint in nanoERG, display tokens for burn -> raw)
  const mintNanoErg = Math.floor(parseFloat(mintAmountStr || '0') * 1e9)
  const burnAmount = Math.floor(parseFloat(burnAmountStr || '0') * hodlDivisor)

  const handlePreview = useCallback(async () => {
    setError(null)
    setLoading(true)
    try {
      if (tab === 'mint') {
        if (mintNanoErg <= 0) {
          setError('Enter an amount to deposit')
          return
        }
        const preview = await previewHodlCoinMint(bank.singletonTokenId, mintNanoErg)
        setMintPreview(preview)
      } else {
        if (burnAmount <= 0) {
          setError('Enter an amount to burn')
          return
        }
        const preview = await previewHodlCoinBurn(bank.singletonTokenId, burnAmount)
        setBurnPreview(preview)
      }
      setStep('preview')
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [tab, mintNanoErg, burnAmount, bank.singletonTokenId])

  const handleBuild = async () => {
    setLoading(true)
    setError(null)
    setStep('building')

    try {
      const utxos = await invoke<Array<{ ergo_tree?: string; ergoTree?: string }>>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      let unsignedTx: object
      let message: string

      if (tab === 'mint') {
        unsignedTx = await buildHodlCoinMintTx(
          bank.singletonTokenId,
          mintNanoErg,
          utxos as object[],
          nodeStatus.chain_height,
        )
        message = `Mint ${bankName}: deposit ${formatNanoErg(mintNanoErg)} ERG`
      } else {
        unsignedTx = await buildHodlCoinBurnTx(
          bank.singletonTokenId,
          burnAmount,
          utxos as object[],
          nodeStatus.chain_height,
        )
        message = `Burn ${burnAmount} ${bankName}`
      }

      const signResult = await startHodlCoinSign(unsignedTx, message)

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const handleReset = () => {
    setStep('input')
    setError(null)
    setMintAmountStr('')
    setBurnAmountStr('')
    setMintPreview(null)
    setBurnPreview(null)
    flow.reset()
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="hodl-modal" onClick={e => e.stopPropagation()}>
        <div className="hodl-modal-header">
          <h2>{bankName}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Tab switcher - only on input step */}
        {step === 'input' && (
          <div className="hodl-tabs">
            <button
              className={`hodl-tab-btn ${tab === 'mint' ? 'active' : ''}`}
              onClick={() => { setTab('mint'); setError(null) }}
            >
              Mint
            </button>
            <button
              className={`hodl-tab-btn ${tab === 'burn' ? 'active' : ''}`}
              onClick={() => { setTab('burn'); setError(null) }}
            >
              Burn (Redeem)
            </button>
          </div>
        )}

        <div className="hodl-modal-content">
          {/* Input Step */}
          {step === 'input' && tab === 'mint' && (
            <>
              <div className="hodl-input-group">
                <label>Deposit ERG</label>
                <div className="hodl-input-wrapper">
                  <input
                    type="text"
                    inputMode="decimal"
                    placeholder="0.0"
                    value={mintAmountStr}
                    onChange={e => setMintAmountStr(e.target.value)}
                  />
                  <button
                    className="hodl-max-btn"
                    onClick={() => {
                      const max = Math.max(0, ergBalance - 5_000_000) / 1e9
                      setMintAmountStr(max.toFixed(4))
                    }}
                  >
                    Max
                  </button>
                </div>
                <div className="hodl-input-hint">
                  Balance: {formatNanoErg(ergBalance)} ERG
                </div>
              </div>
              <div className="hodl-info-row">
                <span>Price</span>
                <span>{formatNanoErg(bank.priceNanoPerHodl * 1e9)} ERG per token</span>
              </div>
              <div className="hodl-info-row">
                <span>Est. tokens</span>
                <span>{mintNanoErg > 0
                  ? Math.floor(mintNanoErg / (bank.priceNanoPerHodl * 1e9) || 0).toLocaleString()
                  : '0'
                }</span>
              </div>
            </>
          )}

          {step === 'input' && tab === 'burn' && (
            <>
              <div className="hodl-input-group">
                <label>Burn {bankName}</label>
                <div className="hodl-input-wrapper">
                  <input
                    type="text"
                    inputMode="decimal"
                    placeholder="0"
                    value={burnAmountStr}
                    onChange={e => setBurnAmountStr(e.target.value)}
                  />
                  <button
                    className="hodl-max-btn"
                    onClick={() => setBurnAmountStr(hodlBalanceDisplay.toString())}
                  >
                    Max
                  </button>
                </div>
                <div className="hodl-input-hint">
                  Balance: {hodlBalanceDisplay.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 })} {bankName}
                </div>
              </div>
              <div className="hodl-info-row">
                <span>Price</span>
                <span>{formatNanoErg(bank.priceNanoPerHodl * 1e9)} ERG per token</span>
              </div>
              <div className="hodl-info-row">
                <span>Total fee</span>
                <span>{bank.totalFeePct.toFixed(1)}% (bank {bank.bankFeePct.toFixed(1)}% + dev {bank.devFeePct.toFixed(1)}%)</span>
              </div>
              <div className="hodl-info-row">
                <span>Est. ERG received</span>
                <span>{burnAmount > 0
                  ? formatNanoErg(Math.floor(burnAmount * bank.priceNanoPerHodl * 1e9 * (1 - bank.totalFeePct / 100)))
                  : '0'
                } ERG</span>
              </div>
            </>
          )}

          {step === 'input' && error && <div className="message error">{error}</div>}

          {step === 'input' && (
            <button
              className="btn btn-primary hodl-submit-btn"
              onClick={handlePreview}
              disabled={loading || (tab === 'mint' ? mintNanoErg <= 0 : burnAmount <= 0)}
            >
              {loading ? 'Loading...' : 'Preview'}
            </button>
          )}

          {/* Preview Step */}
          {step === 'preview' && tab === 'mint' && mintPreview && (
            <>
              <div className="hodl-preview-section">
                <h3>Mint Preview</h3>
                <div className="hodl-info-row">
                  <span>Deposit</span>
                  <span>{formatNanoErg(mintPreview.ergDeposited)} ERG</span>
                </div>
                <div className="hodl-info-row highlight">
                  <span>You Receive</span>
                  <span>{mintPreview.hodlTokensReceived.toLocaleString()} {bankName}</span>
                </div>
                <div className="hodl-info-row">
                  <span>Price</span>
                  <span>{formatNanoErg(mintPreview.pricePerToken * 1e9)} ERG</span>
                </div>
                <div className="hodl-info-row">
                  <span>Miner Fee</span>
                  <span>{formatNanoErg(mintPreview.minerFee)} ERG</span>
                </div>
                <div className="hodl-info-row total">
                  <span>Total Cost</span>
                  <span>{formatNanoErg(mintPreview.totalErgCost)} ERG</span>
                </div>
              </div>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={() => setStep('input')}>Back</button>
                <button className="btn btn-primary" onClick={handleBuild} disabled={loading}>
                  {loading ? 'Building...' : 'Confirm Mint'}
                </button>
              </div>
            </>
          )}

          {step === 'preview' && tab === 'burn' && burnPreview && (
            <>
              <div className="hodl-preview-section">
                <h3>Burn Preview</h3>
                <div className="hodl-info-row">
                  <span>Burning</span>
                  <span>{burnPreview.hodlTokensSpent.toLocaleString()} {bankName}</span>
                </div>
                <div className="hodl-info-row">
                  <span>Gross Value</span>
                  <span>{formatNanoErg(burnPreview.ergBeforeFees)} ERG</span>
                </div>
                <div className="hodl-info-row fee">
                  <span>Bank Fee ({bank.bankFeePct.toFixed(1)}%)</span>
                  <span>-{formatNanoErg(burnPreview.bankFeeNano)} ERG</span>
                </div>
                <div className="hodl-info-row fee">
                  <span>Dev Fee ({bank.devFeePct.toFixed(1)}%)</span>
                  <span>-{formatNanoErg(burnPreview.devFeeNano)} ERG</span>
                </div>
                <div className="hodl-info-row highlight">
                  <span>You Receive</span>
                  <span>{formatNanoErg(burnPreview.ergReceived)} ERG</span>
                </div>
                <div className="hodl-info-row">
                  <span>Miner Fee</span>
                  <span>{formatNanoErg(burnPreview.minerFee)} ERG</span>
                </div>
              </div>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={() => setStep('input')}>Back</button>
                <button className="btn btn-primary" onClick={handleBuild} disabled={loading}>
                  {loading ? 'Building...' : 'Confirm Burn'}
                </button>
              </div>
            </>
          )}

          {/* Building Step */}
          {step === 'building' && (
            <div className="hodl-centered">
              <div className="spinner-small" />
              <span>Building transaction...</span>
            </div>
          )}

          {/* Signing Step - Choose */}
          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="mint-signing-step">
              <p>Choose your signing method</p>
              <div className="wallet-options">
                <button className="wallet-option" onClick={flow.handleNautilusSign}>
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
              <p>Approve in Nautilus</p>
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
                <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                <button className="btn btn-primary" onClick={flow.handleNautilusSign}>Open Nautilus Again</button>
              </div>
            </div>
          )}

          {/* Signing Step - QR */}
          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div className="mint-signing-step">
              <p>Scan with your Ergo wallet</p>
              <div className="qr-container">
                <QRCodeSVG value={flow.qrUrl} size={200} />
              </div>
              <p className="signing-hint">Waiting for signature...</p>
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
            </div>
          )}

          {/* Success */}
          {step === 'success' && flow.txId && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>{tab === 'mint' ? 'Mint' : 'Burn'} Successful!</h3>
              <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
              <button className="btn btn-primary" onClick={onClose}>Done</button>
            </div>
          )}

          {/* Error */}
          {step === 'error' && (
            <div className="error-step">
              <div className="error-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                </svg>
              </div>
              <h3>Transaction Failed</h3>
              <p className="error-message">{error}</p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleReset}>Start Over</button>
                <button className="btn btn-primary" onClick={() => { setStep('preview'); setError(null) }}>
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
