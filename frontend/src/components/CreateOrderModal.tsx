import { useState, useEffect, useCallback, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildOpenOrder,
  startSigmaFiSign,
  getSigmaFiTxStatus,
  getSupportedTokens,
  formatAmount,
  type LoanToken,
} from '../api/sigmafi'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'
import { TX_FEE_NANO, MIN_BOX_VALUE_NANO } from '../constants'
import './CreateOrderModal.css'

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

interface CreateOrderModalProps {
  isOpen: boolean
  onClose: () => void
  onSuccess: () => void
  walletAddress: string
  walletBalance: WalletBalance | null
  explorerUrl: string
}

type ModalStep = 'input' | 'preview' | 'signing' | 'success' | 'error'

const BLOCKS_PER_DAY = 720 // ~2 min blocks

function pollSigmaFiStatus(requestId: string): Promise<TxStatusResponse> {
  return getSigmaFiTxStatus(requestId)
}

export function CreateOrderModal({
  isOpen,
  onClose,
  onSuccess,
  walletBalance,
  explorerUrl,
}: CreateOrderModalProps) {
  const [step, setStep] = useState<ModalStep>('input')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [tokens, setTokens] = useState<LoanToken[]>([])

  // Form state
  const [selectedTokenId, setSelectedTokenId] = useState('')
  const [principalInput, setPrincipalInput] = useState('')
  const [interestInput, setInterestInput] = useState('')
  const [termDays, setTermDays] = useState('')
  const [collateralInput, setCollateralInput] = useState('')

  const flow = useTransactionFlow({
    pollStatus: pollSigmaFiStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'SigmaFi', operation: 'open_order', description: 'Create SigmaFi loan request' },
  })

  // Load supported tokens
  useEffect(() => {
    if (isOpen) {
      getSupportedTokens()
        .then(t => {
          setTokens(t)
          if (t.length > 0 && !selectedTokenId) {
            // Default to SigUSD if available, else first token
            const sigusd = t.find(tk => tk.name === 'SigUSD')
            setSelectedTokenId(sigusd?.token_id || t[0].token_id)
          }
        })
        .catch(e => console.error('Failed to load tokens:', e))
    }
  }, [isOpen])

  // Reset on open
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setLoading(false)
      setError(null)
      setPrincipalInput('')
      setInterestInput('')
      setTermDays('')
      setCollateralInput('')
    }
  }, [isOpen])

  const selectedToken = useMemo(
    () => tokens.find(t => t.token_id === selectedTokenId),
    [tokens, selectedTokenId],
  )

  const calculated = useMemo(() => {
    const decimals = selectedToken?.decimals ?? 0
    const multiplier = Math.pow(10, decimals)
    const principal = parseFloat(principalInput) || 0
    const interest = parseFloat(interestInput) || 0
    const days = parseFloat(termDays) || 0
    const collateralErg = parseFloat(collateralInput) || 0

    const repaymentFloat = principal * (1 + interest / 100)
    const principalRaw = Math.round(principal * multiplier)
    const repaymentRaw = Math.round(repaymentFloat * multiplier)
    const collateralNano = Math.round(collateralErg * 1e9)
    const maturityBlocks = Math.round(days * BLOCKS_PER_DAY)
    const apr = days > 0 ? (interest / days) * 365 : 0

    // ERG needed: collateral + tx fee + min box value
    const ergNeeded = collateralNano + TX_FEE_NANO + MIN_BOX_VALUE_NANO

    const isValid = principal > 0 && interest > 0 && days > 0 && collateralErg > 0
      && maturityBlocks >= 30
      && ergNeeded <= (walletBalance?.erg_nano ?? 0)

    return {
      principal,
      principalRaw,
      repaymentFloat,
      repaymentRaw,
      interest,
      collateralErg,
      collateralNano,
      maturityBlocks,
      days,
      apr,
      ergNeeded,
      isValid,
    }
  }, [principalInput, interestInput, termDays, collateralInput, selectedToken, walletBalance])

  const handleBuild = useCallback(async () => {
    if (!calculated.isValid || !selectedToken) {
      setError('Please fill all fields with valid values')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const utxos = await invoke<object[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const userErgoTree = (utxos[0] as { ergoTree: string }).ergoTree

      const unsignedTx = await buildOpenOrder(
        userErgoTree,
        selectedToken.token_id,
        String(calculated.principalRaw),
        String(calculated.repaymentRaw),
        calculated.maturityBlocks,
        String(calculated.collateralNano),
        '[]',
        utxos,
        nodeStatus.chain_height,
      )

      const signResult = await startSigmaFiSign(
        unsignedTx,
        `Create loan request: ${calculated.principal} ${selectedToken.name}`,
      )
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }, [calculated, selectedToken, flow])

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal create-order-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Create Loan Request</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="co-input-step">
              {/* Token selector */}
              <div className="form-group">
                <label className="form-label">Loan Token</label>
                <select
                  className="co-select"
                  value={selectedTokenId}
                  onChange={e => setSelectedTokenId(e.target.value)}
                >
                  {tokens.map(t => (
                    <option key={t.token_id} value={t.token_id}>{t.name}</option>
                  ))}
                </select>
              </div>

              {/* Principal */}
              <div className="form-group">
                <label className="form-label">Principal ({selectedToken?.name ?? ''})</label>
                <input
                  type="number"
                  className="input"
                  value={principalInput}
                  onChange={e => setPrincipalInput(e.target.value)}
                  placeholder="0"
                  min="0"
                  step={selectedToken ? Math.pow(10, -selectedToken.decimals) : 1}
                />
              </div>

              {/* Interest */}
              <div className="form-group">
                <label className="form-label">Interest (%)</label>
                <input
                  type="number"
                  className="input"
                  value={interestInput}
                  onChange={e => setInterestInput(e.target.value)}
                  placeholder="5"
                  min="0.1"
                  step="0.1"
                />
                {calculated.interest > 0 && calculated.principal > 0 && (
                  <p className="co-hint">
                    Repayment: {calculated.repaymentFloat.toLocaleString(undefined, {
                      minimumFractionDigits: 2,
                      maximumFractionDigits: selectedToken?.decimals ?? 2,
                    })} {selectedToken?.name}
                  </p>
                )}
              </div>

              {/* Term */}
              <div className="form-group">
                <label className="form-label">Term (days)</label>
                <input
                  type="number"
                  className="input"
                  value={termDays}
                  onChange={e => setTermDays(e.target.value)}
                  placeholder="30"
                  min="1"
                  step="1"
                />
                {calculated.days > 0 && calculated.interest > 0 && (
                  <p className="co-hint">
                    APR: {calculated.apr.toLocaleString(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}%
                    &middot; {calculated.maturityBlocks.toLocaleString()} blocks
                  </p>
                )}
              </div>

              {/* Collateral */}
              <div className="form-group">
                <label className="form-label">Collateral (ERG)</label>
                <input
                  type="number"
                  className="input"
                  value={collateralInput}
                  onChange={e => setCollateralInput(e.target.value)}
                  placeholder="0"
                  min="0.001"
                  step="0.001"
                />
                <p className="co-hint">
                  Available: {formatErg(walletBalance?.erg_nano ?? 0)} ERG
                  {calculated.collateralNano > 0 && (
                    <> &middot; Total needed: {formatErg(calculated.ergNeeded)} ERG</>
                  )}
                </p>
              </div>

              {/* Summary preview */}
              {calculated.isValid && (
                <div className="co-summary">
                  <div className="co-summary-row">
                    <span>You request</span>
                    <span>{formatAmount(calculated.principalRaw, selectedToken?.decimals ?? 0)} {selectedToken?.name}</span>
                  </div>
                  <div className="co-summary-row">
                    <span>You repay</span>
                    <span>{formatAmount(calculated.repaymentRaw, selectedToken?.decimals ?? 0)} {selectedToken?.name}</span>
                  </div>
                  <div className="co-summary-row">
                    <span>You lock</span>
                    <span>{formatErg(calculated.collateralNano)} ERG</span>
                  </div>
                </div>
              )}

              {calculated.maturityBlocks > 0 && calculated.maturityBlocks < 30 && (
                <div className="message warning">Minimum term is 30 blocks (~1 hour)</div>
              )}
              {calculated.ergNeeded > (walletBalance?.erg_nano ?? 0) && calculated.collateralNano > 0 && (
                <div className="message warning">Insufficient ERG balance</div>
              )}

              {error && <div className="message error">{error}</div>}

              <div className="co-modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
                <button
                  className="btn btn-primary"
                  onClick={handleBuild}
                  disabled={loading || !calculated.isValid}
                >
                  {loading ? 'Building...' : 'Create Order'}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && (
            <div className="co-signing-step">
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
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" />
                      <path d="M12 17v4" />
                    </svg>
                  </div>
                  <p>Approve in Nautilus</p>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                </div>
              )}

              {flow.signMethod === 'mobile' && flow.qrUrl && (
                <div className="qr-signing">
                  <p>Scan with Ergo Mobile Wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG value={flow.qrUrl} size={200} level="M" includeMargin bgColor="white" fgColor="black" />
                  </div>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                </div>
              )}
            </div>
          )}

          {step === 'success' && (
            <div className="co-success-step">
              <div className="success-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Order Created!</h3>
              <p>Your loan request has been submitted. Lenders can now fill it.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { onSuccess(); onClose() }}>Done</button>
            </div>
          )}

          {step === 'error' && (
            <div className="co-error-step">
              <div className="error-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--red-400)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="15" y1="9" x2="9" y2="15" />
                  <line x1="9" y1="9" x2="15" y2="15" />
                </svg>
              </div>
              <h3>Transaction Failed</h3>
              <p className="error-message">{error}</p>
              <div className="co-modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>Close</button>
                <button className="btn btn-primary" onClick={() => setStep('input')}>Try Again</button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
