/**
 * RepayModal Component
 *
 * Modal for repaying borrowed funds from Duckpools lending pools.
 * Displays borrow position info, repay amount input, new health factor preview,
 * fee breakdown, and handles transaction building and signing via Nautilus or ErgoPay.
 */

import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildRepayTx,
  formatAmount,
  formatApy,
  getHealthStatus,
  type PoolInfo,
  type BorrowPositionInfo,
  type LendingBuildResponse,
} from '../api/lending'
import { TX_FEE_NANO } from '../constants'
import { formatErg } from '../utils/format'
import type { WalletBalance } from './MarketCard'
import { TxSuccess } from './TxSuccess'
import './LendModal.css' // Reuse LendModal styles

interface RepayModalProps {
  /** Whether the modal is open */
  isOpen: boolean
  /** Callback to close the modal */
  onClose: () => void
  /** The pool to repay to */
  pool: PoolInfo
  /** User's borrow position in this pool */
  borrowPosition: BorrowPositionInfo
  /** User's wallet address */
  userAddress: string
  /** Wallet balance information */
  walletBalance: WalletBalance | null
  /** Explorer URL for transaction links */
  explorerUrl: string
  /** Callback when transaction succeeds */
  onSuccess: () => void
}

type TxStep = 'input' | 'preview' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'

export function RepayModal({
  isOpen,
  onClose,
  pool,
  borrowPosition,
  userAddress,
  walletBalance,
  explorerUrl,
  onSuccess,
}: RepayModalProps) {
  // Step state
  const [step, setStep] = useState<TxStep>('input')
  const [inputValue, setInputValue] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Build response
  const [buildResponse, setBuildResponse] = useState<LendingBuildResponse | null>(null)

  // Signing state
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)

  // Get total owed amount (in raw units)
  const totalOwed = useMemo(() => {
    try {
      return Number(BigInt(borrowPosition.total_owed))
    } catch {
      return 0
    }
  }, [borrowPosition])

  // Get user's available balance for repayment (pool's asset)
  const availableBalance = useMemo(() => {
    if (!walletBalance) return 0

    if (pool.is_erg_pool) {
      return walletBalance.erg_nano
    }

    // For token pools, find the token
    const token = walletBalance.tokens.find((t) => {
      return (
        t.name?.toLowerCase() === pool.symbol.toLowerCase() ||
        t.name?.toLowerCase() === pool.name.toLowerCase()
      )
    })

    return token?.amount || 0
  }, [walletBalance, pool])

  // Calculate amounts based on input
  const calculated = useMemo(() => {
    if (!inputValue) {
      return {
        amount: 0,
        amountRaw: 0,
        txFee: TX_FEE_NANO,
        isValid: false,
        isFullRepay: false,
        newHealthFactor: borrowPosition.health_factor,
        remainingDebt: totalOwed,
      }
    }

    const value = parseFloat(inputValue)
    if (isNaN(value) || value <= 0) {
      return {
        amount: 0,
        amountRaw: 0,
        txFee: TX_FEE_NANO,
        isValid: false,
        isFullRepay: false,
        newHealthFactor: borrowPosition.health_factor,
        remainingDebt: totalOwed,
      }
    }

    const multiplier = Math.pow(10, pool.decimals)
    const amountRaw = Math.round(value * multiplier)

    const txFee = TX_FEE_NANO

    // Calculate remaining debt after repayment
    const remainingDebt = Math.max(0, totalOwed - amountRaw)

    // Check if this is a full repayment (or more than owed)
    const isFullRepay = amountRaw >= totalOwed

    // Estimate new health factor after repayment
    // Health factor = (collateral_value * liquidation_threshold) / borrowed_amount
    // If remaining debt is 0, health factor is effectively infinite
    let newHealthFactor: number
    if (remainingDebt === 0) {
      newHealthFactor = Infinity
    } else {
      // Simplified calculation: scale health factor proportionally
      // Current: HF = collateral * threshold / total_owed
      // New: HF = collateral * threshold / remaining_debt
      // So: new_HF = current_HF * (total_owed / remaining_debt)
      newHealthFactor = borrowPosition.health_factor * (totalOwed / remainingDebt)
    }

    // Validation
    const hasEnoughAsset = amountRaw <= availableBalance
    const hasEnoughErgForFee = pool.is_erg_pool
      ? (amountRaw + txFee) <= (walletBalance?.erg_nano || 0)
      : txFee <= (walletBalance?.erg_nano || 0)
    const repaysAtLeastSomething = amountRaw > 0

    return {
      amount: value,
      amountRaw,
      txFee,
      isValid: repaysAtLeastSomething && hasEnoughAsset && hasEnoughErgForFee,
      isFullRepay,
      newHealthFactor,
      remainingDebt,
      hasEnoughAsset,
      hasEnoughErgForFee,
    }
  }, [inputValue, pool, totalOwed, availableBalance, walletBalance, borrowPosition.health_factor])

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setInputValue('')
      setLoading(false)
      setError(null)
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

  // Build transaction and show preview
  const handleBuild = useCallback(async () => {
    if (!calculated.isValid) {
      setError('Please enter a valid amount')
      return
    }

    setLoading(true)
    setError(null)

    try {
      // Get user UTXOs (fetch fresh UTXOs for the transaction)
      const utxos = await invoke<unknown[]>('get_user_utxos')

      // Get current height
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      // Build the repay transaction
      const response = await buildRepayTx({
        pool_id: pool.pool_id,
        collateral_box_id: borrowPosition.collateral_box_id,
        repay_amount: calculated.amountRaw,
        user_address: userAddress,
        user_utxos: utxos,
        current_height: nodeStatus.chain_height,
      })

      setBuildResponse(response)
      setStep('preview')
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [calculated, pool.pool_id, borrowPosition.collateral_box_id, userAddress])

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
          message: `Repay ${calculated.amount.toFixed(pool.decimals)} ${pool.symbol} to ${pool.name}`,
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
  }, [buildResponse, calculated.amount, pool])

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

  // Handle max button click (repay full amount)
  const handleMaxClick = useCallback(() => {
    if (!walletBalance || totalOwed <= 0) return

    // Calculate max repayable amount
    let maxRepay: number
    if (pool.is_erg_pool) {
      // For ERG pools, we need ERG for both repayment and fee
      const buffer = 10000000 // 0.01 ERG buffer
      const maxAvailable = Math.max(0, walletBalance.erg_nano - TX_FEE_NANO - buffer)
      maxRepay = Math.min(totalOwed, maxAvailable)
    } else {
      // For token pools, just limited by token balance and total owed
      maxRepay = Math.min(totalOwed, availableBalance)
    }

    const displayAmount = maxRepay / Math.pow(10, pool.decimals)
    setInputValue(displayAmount.toFixed(pool.decimals))
  }, [walletBalance, pool, totalOwed, availableBalance])

  const formatDisplayAmount = (value: number) => {
    if (pool.decimals === 0) {
      return value.toLocaleString(undefined, { maximumFractionDigits: 0 })
    }
    return value.toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: pool.decimals,
    })
  }

  const formatHealthFactor = (hf: number) => {
    if (!isFinite(hf)) return 'N/A (No debt)'
    return hf.toFixed(2)
  }

  const getHealthFactorClass = (hf: number) => {
    if (!isFinite(hf)) return 'green'
    return getHealthStatus(hf)
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal lend-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Repay {pool.symbol}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="lend-input-step">
              {/* Pool Info */}
              <div className="pool-info-card">
                <div className="pool-info-row">
                  <span className="pool-info-label">Pool</span>
                  <span className="pool-info-value">{pool.name}</span>
                </div>
                <div className="pool-info-row">
                  <span className="pool-info-label">Borrow APY</span>
                  <span className="pool-info-value">{formatApy(pool.borrow_apy)}</span>
                </div>
              </div>

              {/* Borrow Position Info */}
              <div className={`pool-info-card borrow-position ${borrowPosition.health_status}`}>
                <div className="pool-info-row">
                  <span className="pool-info-label">Borrowed Amount</span>
                  <span className="pool-info-value">
                    {formatAmount(borrowPosition.borrowed_amount, pool.decimals)} {pool.symbol}
                  </span>
                </div>
                <div className="pool-info-row">
                  <span className="pool-info-label">Total Owed (with interest)</span>
                  <span className="pool-info-value">
                    {formatAmount(borrowPosition.total_owed, pool.decimals)} {pool.symbol}
                  </span>
                </div>
                <div className="pool-info-row">
                  <span className="pool-info-label">Collateral</span>
                  <span className="pool-info-value">
                    {formatAmount(borrowPosition.collateral_amount, 9)} {borrowPosition.collateral_name || 'ERG'}
                  </span>
                </div>
                <div className="pool-info-row">
                  <span className="pool-info-label">Current Health Factor</span>
                  <span className={`pool-info-value health-${borrowPosition.health_status}`}>
                    {borrowPosition.health_factor.toFixed(2)}
                    {borrowPosition.health_status === 'red' && (
                      <span className="health-warning" title="At risk of liquidation"> !</span>
                    )}
                  </span>
                </div>
              </div>

              {/* Amount Input */}
              <div className="form-group">
                <label className="form-label">Amount to Repay</label>
                <div className="input-with-max">
                  <input
                    type="number"
                    className="input"
                    value={inputValue}
                    onChange={(e) => setInputValue(e.target.value)}
                    placeholder="0"
                    min="0"
                    step={Math.pow(10, -pool.decimals)}
                  />
                  <div className="input-suffix">
                    <span className="input-currency">{pool.symbol}</span>
                    <button className="max-btn" onClick={handleMaxClick} type="button">
                      MAX
                    </button>
                  </div>
                </div>
                <p className="balance-hint">
                  Available: {formatDisplayAmount(availableBalance / Math.pow(10, pool.decimals))} {pool.symbol}
                </p>
              </div>

              {/* Repayment Preview */}
              {inputValue && calculated.amount > 0 && (
                <div className="calculated-output">
                  <div className="output-row">
                    <span className="output-label">Repay Amount</span>
                    <span className="output-value">
                      {formatDisplayAmount(calculated.amount)} {pool.symbol}
                    </span>
                  </div>
                  {calculated.isFullRepay && (
                    <div className="output-row highlight">
                      <span className="output-label">Full Repayment</span>
                      <span className="output-value positive">
                        Collateral will be returned
                      </span>
                    </div>
                  )}
                  {!calculated.isFullRepay && (
                    <>
                      <div className="output-row">
                        <span className="output-label">Remaining Debt</span>
                        <span className="output-value">
                          {formatDisplayAmount(calculated.remainingDebt / Math.pow(10, pool.decimals))} {pool.symbol}
                        </span>
                      </div>
                      <div className="output-row">
                        <span className="output-label">New Health Factor</span>
                        <span className={`output-value health-${getHealthFactorClass(calculated.newHealthFactor)}`}>
                          {borrowPosition.health_factor.toFixed(2)} -&gt; {formatHealthFactor(calculated.newHealthFactor)}
                        </span>
                      </div>
                    </>
                  )}
                  <div className="output-row muted">
                    <span>Transaction Fee</span>
                    <span>{formatErg(TX_FEE_NANO)} ERG</span>
                  </div>
                </div>
              )}

              {/* Validation Warnings */}
              {inputValue && calculated.amount > 0 && (
                <>
                  {!calculated.hasEnoughAsset && (
                    <div className="message warning">
                      Insufficient {pool.symbol} balance
                    </div>
                  )}
                  {!calculated.hasEnoughErgForFee && (
                    <div className="message warning">
                      Insufficient ERG for transaction fee
                    </div>
                  )}
                </>
              )}

              {error && <div className="message error">{error}</div>}

              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleBuild}
                  disabled={loading || !calculated.isValid}
                >
                  {loading ? 'Building...' : 'Build Transaction'}
                </button>
              </div>
            </div>
          )}

          {step === 'preview' && buildResponse && (
            <div className="lend-preview-step">
              <div className="preview-summary">
                <div className="preview-header">
                  <span className="preview-label">You Will Repay</span>
                  <span className="preview-value">
                    {formatDisplayAmount(calculated.amount)} {pool.symbol}
                  </span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>Repay Amount</span>
                    <span>
                      {buildResponse.summary.amount_in} {pool.symbol}
                    </span>
                  </div>
                  {calculated.isFullRepay && (
                    <div className="detail-row highlight">
                      <span>Full Repayment</span>
                      <span className="positive">Yes - Collateral Returned</span>
                    </div>
                  )}
                  {!calculated.isFullRepay && (
                    <div className="detail-row">
                      <span>New Health Factor</span>
                      <span className={`health-${getHealthFactorClass(calculated.newHealthFactor)}`}>
                        {formatHealthFactor(calculated.newHealthFactor)}
                      </span>
                    </div>
                  )}
                  <div className="detail-row">
                    <span>Transaction Fee</span>
                    <span>{formatErg(Number(buildResponse.summary.tx_fee_nano))} ERG</span>
                  </div>
                  <div className="detail-row">
                    <span>Refund Height</span>
                    <span>{buildResponse.summary.refund_height.toLocaleString()}</span>
                  </div>
                </div>

                <p className="preview-note">
                  {calculated.isFullRepay
                    ? `The Duckpools bot will process your repayment and return your collateral (${formatAmount(borrowPosition.collateral_amount, 9)} ${borrowPosition.collateral_name || 'ERG'}) to your wallet.`
                    : `The Duckpools bot will process your partial repayment. Your remaining debt will be ${formatDisplayAmount(calculated.remainingDebt / Math.pow(10, pool.decimals))} ${pool.symbol}.`}
                  {` If not processed by block ${buildResponse.summary.refund_height.toLocaleString()}, you can reclaim your funds.`}
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
              <h3>Transaction Submitted!</h3>
              <p>Your repay transaction has been submitted to the network.</p>
              <p className="success-note">
                {calculated.isFullRepay
                  ? 'The Duckpools bot will process your repayment and return your collateral to your wallet.'
                  : 'The Duckpools bot will process your partial repayment. Your health factor will improve.'}
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
              <h3>Transaction Failed</h3>
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

export default RepayModal
