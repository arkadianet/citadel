/**
 * WithdrawModal Component
 *
 * Modal for redeeming LP tokens from Duckpools lending pools.
 * Displays pool info, user's lending position, LP amount input,
 * expected underlying assets to receive, fee breakdown, and
 * handles transaction building and signing via Nautilus or ErgoPay.
 */

import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildWithdrawTx,
  formatAmount,
  formatApy,
  type PoolInfo,
  type LendPositionInfo,
  type LendingBuildResponse,
} from '../api/lending'
import { TX_FEE_NANO } from '../constants'
import { formatErg } from '../utils/format'
import type { WalletBalance } from './MarketCard'
import { TxSuccess } from './TxSuccess'
import './LendModal.css' // Reuse LendModal styles

interface WithdrawModalProps {
  /** Whether the modal is open */
  isOpen: boolean
  /** Callback to close the modal */
  onClose: () => void
  /** The pool to withdraw from */
  pool: PoolInfo
  /** User's lending position in this pool */
  lendPosition: LendPositionInfo | undefined
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

export function WithdrawModal({
  isOpen,
  onClose,
  pool,
  lendPosition,
  userAddress,
  walletBalance,
  explorerUrl,
  onSuccess,
}: WithdrawModalProps) {
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

  // Get user's LP token balance (in raw units)
  const lpTokenBalance = useMemo(() => {
    if (!lendPosition) return 0
    try {
      return Number(BigInt(lendPosition.lp_tokens))
    } catch {
      return 0
    }
  }, [lendPosition])

  // Get underlying value of user's position
  const underlyingValue = useMemo(() => {
    if (!lendPosition) return '0'
    return lendPosition.underlying_value
  }, [lendPosition])

  // Calculate amounts based on input
  const calculated = useMemo(() => {
    if (!inputValue) {
      return { amount: 0, amountRaw: 0, txFee: TX_FEE_NANO, estimatedReturn: 0, isValid: false }
    }

    const value = parseFloat(inputValue)
    if (isNaN(value) || value <= 0) {
      return { amount: 0, amountRaw: 0, txFee: TX_FEE_NANO, estimatedReturn: 0, isValid: false }
    }

    const multiplier = Math.pow(10, pool.decimals)
    const amountRaw = Math.round(value * multiplier)

    const txFee = TX_FEE_NANO

    // Calculate estimated return based on LP token ratio
    // estimatedReturn = (lpAmount / totalLpTokens) * underlyingValue
    // For simplicity, we assume LP tokens track 1:1 with underlying during withdraw
    const estimatedReturn = amountRaw

    // Validation
    const hasEnoughLpTokens = amountRaw <= lpTokenBalance
    const hasEnoughErgForFee = (walletBalance?.erg_nano || 0) >= txFee

    return {
      amount: value,
      amountRaw,
      txFee,
      estimatedReturn,
      isValid: value > 0 && hasEnoughLpTokens && hasEnoughErgForFee,
      hasEnoughLpTokens,
      hasEnoughErgForFee,
    }
  }, [inputValue, pool.decimals, lpTokenBalance, walletBalance])

  // Estimate underlying assets to receive
  const estimatedUnderlying = useMemo(() => {
    if (!calculated.isValid || calculated.amountRaw <= 0 || !lendPosition) return '0'

    // Calculate proportional underlying value
    // underlying = (lpAmount / totalLpTokens) * underlyingValue
    try {
      const lpAmount = BigInt(calculated.amountRaw)
      const totalLp = BigInt(lendPosition.lp_tokens)
      const totalUnderlying = BigInt(lendPosition.underlying_value)

      if (totalLp === 0n) return '0'

      const estimated = (lpAmount * totalUnderlying) / totalLp
      return formatAmount(estimated.toString(), pool.decimals)
    } catch {
      return formatAmount(calculated.amountRaw.toString(), pool.decimals)
    }
  }, [calculated, lendPosition, pool.decimals])

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

      // Build the withdraw transaction
      const response = await buildWithdrawTx({
        pool_id: pool.pool_id,
        lp_amount: calculated.amountRaw,
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
  }, [calculated, pool.pool_id, userAddress])

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
          message: `Withdraw ${calculated.amount.toFixed(pool.decimals)} LP tokens from ${pool.name}`,
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

  // Handle max button click
  const handleMaxClick = useCallback(() => {
    if (!lendPosition || lpTokenBalance <= 0) return

    const displayAmount = lpTokenBalance / Math.pow(10, pool.decimals)
    setInputValue(displayAmount.toFixed(pool.decimals))
  }, [lendPosition, lpTokenBalance, pool.decimals])

  const formatDisplayAmount = (value: number) => {
    if (pool.decimals === 0) {
      return value.toLocaleString(undefined, { maximumFractionDigits: 0 })
    }
    return value.toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: pool.decimals,
    })
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal lend-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Withdraw {pool.symbol}</h2>
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
                  <span className="pool-info-label">Supply APY</span>
                  <span className="pool-info-value positive">{formatApy(pool.supply_apy)}</span>
                </div>
                {lendPosition && (
                  <>
                    <div className="pool-info-row">
                      <span className="pool-info-label">Your LP Tokens</span>
                      <span className="pool-info-value">
                        {formatAmount(lendPosition.lp_tokens, pool.decimals)}
                      </span>
                    </div>
                    <div className="pool-info-row">
                      <span className="pool-info-label">Underlying Value</span>
                      <span className="pool-info-value">
                        {formatAmount(underlyingValue, pool.decimals)} {pool.symbol}
                      </span>
                    </div>
                    {BigInt(lendPosition.unrealized_profit) > 0n && (
                      <div className="pool-info-row">
                        <span className="pool-info-label">Unrealized Profit</span>
                        <span className="pool-info-value positive">
                          +{formatAmount(lendPosition.unrealized_profit, pool.decimals)} {pool.symbol}
                        </span>
                      </div>
                    )}
                  </>
                )}
              </div>

              {/* No Position Warning */}
              {!lendPosition && (
                <div className="message warning">
                  You do not have a lending position in this pool.
                </div>
              )}

              {/* Amount Input */}
              {lendPosition && (
                <>
                  <div className="form-group">
                    <label className="form-label">LP Tokens to Redeem</label>
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
                        <span className="input-currency">LP</span>
                        <button className="max-btn" onClick={handleMaxClick} type="button">
                          MAX
                        </button>
                      </div>
                    </div>
                    <p className="balance-hint">
                      Available: {formatDisplayAmount(lpTokenBalance / Math.pow(10, pool.decimals))} LP tokens
                    </p>
                  </div>

                  {/* Expected Output */}
                  {inputValue && calculated.amount > 0 && (
                    <div className="calculated-output">
                      <div className="output-row">
                        <span className="output-label">Expected to Receive</span>
                        <span className="output-value">~{estimatedUnderlying} {pool.symbol}</span>
                      </div>
                      <div className="output-row muted">
                        <span>Transaction Fee</span>
                        <span>{formatErg(TX_FEE_NANO)} ERG</span>
                      </div>
                    </div>
                  )}

                  {/* Validation Warnings */}
                  {inputValue && calculated.amount > 0 && (
                    <>
                      {!calculated.hasEnoughLpTokens && (
                        <div className="message warning">
                          Insufficient LP token balance
                        </div>
                      )}
                      {!calculated.hasEnoughErgForFee && (
                        <div className="message warning">
                          Insufficient ERG for transaction fee
                        </div>
                      )}
                    </>
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
                  disabled={loading || !calculated.isValid || !lendPosition}
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
                  <span className="preview-label">You Will Withdraw</span>
                  <span className="preview-value">
                    {formatDisplayAmount(calculated.amount)} LP
                  </span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>LP Tokens to Redeem</span>
                    <span>
                      {buildResponse.summary.amount_in}
                    </span>
                  </div>
                  {buildResponse.summary.amount_out_estimate && (
                    <div className="detail-row">
                      <span>Expected {pool.symbol}</span>
                      <span>~{buildResponse.summary.amount_out_estimate}</span>
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
                  The Duckpools bot will process your withdrawal and return {pool.symbol} to your wallet.
                  If not processed by block {buildResponse.summary.refund_height.toLocaleString()}, you can reclaim your LP tokens.
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
              <p>Your withdraw transaction has been submitted to the network.</p>
              <p className="success-note">
                The Duckpools bot will process your withdrawal and return {pool.symbol} to your wallet.
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

export default WithdrawModal
