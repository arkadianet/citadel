/**
 * BorrowModal Component
 *
 * Modal for borrowing assets from Duckpools lending pools.
 * User provides collateral (tokens for ERG pool, ERG for token pools)
 * and specifies the amount to borrow.
 *
 * Follows the same step-based pattern as LendModal:
 * input -> preview -> signing -> success/error
 */

import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildBorrowTx,
  formatAmount,
  formatApy,
  type PoolInfo,
  type LendingBuildResponse,
} from '../api/lending'
import { LENDING_PROXY_FEE_NANO, MIN_BOX_VALUE_NANO } from '../constants'
import { formatErg } from '../utils/format'
import type { WalletBalance } from './MarketCard'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'
import './LendModal.css' // Reuse LendModal styles

interface BorrowModalProps {
  isOpen: boolean
  onClose: () => void
  pool: PoolInfo
  userAddress: string
  walletBalance: WalletBalance | null
  explorerUrl: string
  onSuccess: () => void
}

type TxStep = 'input' | 'preview' | 'signing' | 'success' | 'error'

function pollMintStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}

export function BorrowModal({
  isOpen,
  onClose,
  pool,
  userAddress,
  walletBalance,
  explorerUrl,
  onSuccess,
}: BorrowModalProps) {
  const [step, setStep] = useState<TxStep>('input')
  const [borrowInputValue, setBorrowInputValue] = useState('')
  const [collateralInputValue, setCollateralInputValue] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [buildResponse, setBuildResponse] = useState<LendingBuildResponse | null>(null)

  const flow = useTransactionFlow({
    pollStatus: pollMintStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'Lending', operation: 'borrow', description: `Borrow from ${pool.name} pool` },
  })

  // For token pools, collateral is ERG; for ERG pool, collateral is a token
  // Currently only token pools (borrow tokens, post ERG collateral) are common
  const collateralIsErg = !pool.is_erg_pool
  const collateralSymbol = collateralIsErg ? 'ERG' : 'Collateral Token'
  const collateralDecimals = collateralIsErg ? 9 : pool.decimals

  // Get collateral info from pool
  const collateralOption = pool.collateral_options[0] ?? null
  const hasCollateralOption = collateralOption !== null

  // Available collateral balance
  const availableCollateral = useMemo(() => {
    if (!walletBalance) return 0
    if (collateralIsErg) {
      // Reserve for tx fee
      return Math.max(0, walletBalance.erg_nano - LENDING_PROXY_FEE_NANO - MIN_BOX_VALUE_NANO)
    }
    // Token collateral (ERG pool borrowing) - find the token
    // For now, we'd need to know which token. This path is less common.
    return 0
  }, [walletBalance, collateralIsErg])

  // Available to borrow (pool liquidity)
  const availableLiquidity = useMemo(() => {
    return BigInt(pool.available_liquidity)
  }, [pool.available_liquidity])

  // Calculate amounts
  const calculated = useMemo(() => {
    const empty = {
      borrowAmount: 0,
      borrowAmountRaw: 0,
      collateralAmount: 0,
      collateralAmountRaw: 0,
      txFee: LENDING_PROXY_FEE_NANO,
      isValid: false,
      hasEnoughCollateral: true,
      hasEnoughErgForFee: true,
      borrowExceedsLiquidity: false,
    }
    if (!borrowInputValue || !collateralInputValue) return empty

    const borrowVal = parseFloat(borrowInputValue)
    const collateralVal = parseFloat(collateralInputValue)
    if (isNaN(borrowVal) || borrowVal <= 0 || isNaN(collateralVal) || collateralVal <= 0) return empty

    const borrowMultiplier = Math.pow(10, pool.decimals)
    const borrowAmountRaw = Math.round(borrowVal * borrowMultiplier)

    const collateralMultiplier = Math.pow(10, collateralDecimals)
    const collateralAmountRaw = Math.round(collateralVal * collateralMultiplier)

    const txFee = LENDING_PROXY_FEE_NANO

    // Validation
    const borrowExceedsLiquidity = BigInt(borrowAmountRaw) > availableLiquidity

    let hasEnoughCollateral = true
    let hasEnoughErgForFee = true

    if (collateralIsErg) {
      // Collateral is ERG: need collateral + processing overhead + tx fee + change min
      const totalErgNeeded = collateralAmountRaw + MIN_BOX_VALUE_NANO + txFee + txFee + MIN_BOX_VALUE_NANO
      hasEnoughCollateral = totalErgNeeded <= (walletBalance?.erg_nano || 0)
      hasEnoughErgForFee = hasEnoughCollateral // Same check for ERG collateral
    } else {
      // Collateral is token: need tokens + ERG for processing
      const totalErgNeeded = MIN_BOX_VALUE_NANO + txFee + txFee + MIN_BOX_VALUE_NANO
      hasEnoughErgForFee = totalErgNeeded <= (walletBalance?.erg_nano || 0)
      // Token balance check would go here
    }

    return {
      borrowAmount: borrowVal,
      borrowAmountRaw,
      collateralAmount: collateralVal,
      collateralAmountRaw,
      txFee,
      isValid: borrowVal > 0 && collateralVal > 0 && hasEnoughCollateral && hasEnoughErgForFee && !borrowExceedsLiquidity,
      hasEnoughCollateral,
      hasEnoughErgForFee,
      borrowExceedsLiquidity,
    }
  }, [borrowInputValue, collateralInputValue, pool, collateralDecimals, collateralIsErg, walletBalance, availableLiquidity])

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setBorrowInputValue('')
      setCollateralInputValue('')
      setLoading(false)
      setError(null)
      setBuildResponse(null)
    }
  }, [isOpen])

  // Build transaction
  const handleBuild = useCallback(async () => {
    if (!calculated.isValid) {
      setError('Please enter valid amounts')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const collateralToken = collateralIsErg
        ? 'native'
        : (collateralOption?.token_id || '')

      const response = await buildBorrowTx({
        pool_id: pool.pool_id,
        collateral_token: collateralToken,
        collateral_amount: calculated.collateralAmountRaw,
        borrow_amount: calculated.borrowAmountRaw,
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
  }, [calculated, pool.pool_id, userAddress, collateralIsErg, collateralOption])

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
          message: `Borrow ${calculated.borrowAmount.toFixed(pool.decimals)} ${pool.symbol} from ${pool.name}`,
        },
      })

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }, [buildResponse, calculated.borrowAmount, pool, flow])

  // Handle max collateral button
  const handleMaxCollateral = useCallback(() => {
    if (!walletBalance) return

    if (collateralIsErg) {
      const buffer = 10_000_000 // 0.01 ERG buffer
      const maxCollateral = Math.max(0, walletBalance.erg_nano - LENDING_PROXY_FEE_NANO - MIN_BOX_VALUE_NANO * 2 - LENDING_PROXY_FEE_NANO - buffer)
      const displayAmount = maxCollateral / Math.pow(10, 9)
      setCollateralInputValue(displayAmount.toFixed(9))
    }
  }, [walletBalance, collateralIsErg])

  const formatDisplayAmount = (value: number, decimals: number) => {
    if (decimals === 0) {
      return value.toLocaleString(undefined, { maximumFractionDigits: 0 })
    }
    return value.toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: decimals,
    })
  }

  if (!isOpen) return null

  // If pool has no collateral options, borrowing isn't available
  if (!hasCollateralOption) {
    return (
      <div className="modal-overlay" onClick={onClose}>
        <div className="modal lend-modal" onClick={(e) => e.stopPropagation()}>
          <div className="modal-header">
            <h2>Borrow {pool.symbol}</h2>
            <button className="close-btn" onClick={onClose}>
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>
          <div className="modal-content">
            <div className="message warning">
              Borrowing is not available for the {pool.name}. This pool does not have collateral options configured.
            </div>
            <div className="modal-actions">
              <button className="btn btn-primary" onClick={onClose}>Close</button>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal lend-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Borrow {pool.symbol}</h2>
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
                <div className="pool-info-row">
                  <span className="pool-info-label">Available Liquidity</span>
                  <span className="pool-info-value">
                    {formatAmount(pool.available_liquidity, pool.decimals)} {pool.symbol}
                  </span>
                </div>
                <div className="pool-info-row">
                  <span className="pool-info-label">Liquidation Threshold</span>
                  <span className="pool-info-value">
                    {(collateralOption.liquidation_threshold / 10).toFixed(0)}%
                  </span>
                </div>
              </div>

              {/* Collateral Input */}
              <div className="form-group">
                <label className="form-label">Collateral ({collateralSymbol})</label>
                <div className="input-with-max">
                  <input
                    type="number"
                    className="input"
                    value={collateralInputValue}
                    onChange={(e) => setCollateralInputValue(e.target.value)}
                    placeholder="0"
                    min="0"
                    step={Math.pow(10, -collateralDecimals)}
                  />
                  <div className="input-suffix">
                    <span className="input-currency">{collateralSymbol}</span>
                    {collateralIsErg && (
                      <button className="max-btn" onClick={handleMaxCollateral} type="button">
                        MAX
                      </button>
                    )}
                  </div>
                </div>
                {collateralIsErg && (
                  <p className="balance-hint">
                    Available: {formatDisplayAmount(availableCollateral / 1e9, 4)} ERG
                  </p>
                )}
              </div>

              {/* Borrow Amount Input */}
              <div className="form-group">
                <label className="form-label">Amount to Borrow</label>
                <div className="input-with-max">
                  <input
                    type="number"
                    className="input"
                    value={borrowInputValue}
                    onChange={(e) => setBorrowInputValue(e.target.value)}
                    placeholder="0"
                    min="0"
                    step={Math.pow(10, -pool.decimals)}
                  />
                  <div className="input-suffix">
                    <span className="input-currency">{pool.symbol}</span>
                  </div>
                </div>
                <p className="balance-hint">
                  Pool liquidity: {formatAmount(pool.available_liquidity, pool.decimals)} {pool.symbol}
                </p>
              </div>

              {/* Fee Breakdown */}
              {borrowInputValue && collateralInputValue && calculated.borrowAmount > 0 && (
                <div className="calculated-output">
                  <div className="output-row">
                    <span className="output-label">You Borrow</span>
                    <span className="output-value">
                      {formatDisplayAmount(calculated.borrowAmount, pool.decimals)} {pool.symbol}
                    </span>
                  </div>
                  <div className="output-row muted">
                    <span>Collateral Locked</span>
                    <span>
                      {formatDisplayAmount(calculated.collateralAmount, collateralDecimals)} {collateralSymbol}
                    </span>
                  </div>
                  <div className="output-row muted">
                    <span>Transaction Fee</span>
                    <span>{formatErg(LENDING_PROXY_FEE_NANO)} ERG</span>
                  </div>
                  <div className="output-row muted">
                    <span>Refund Available</span>
                    <span>~24 hours (720 blocks)</span>
                  </div>
                </div>
              )}

              {/* Validation Warnings */}
              {borrowInputValue && collateralInputValue && calculated.borrowAmount > 0 && (
                <>
                  {calculated.borrowExceedsLiquidity && (
                    <div className="message warning">
                      Borrow amount exceeds available pool liquidity
                    </div>
                  )}
                  {!calculated.hasEnoughCollateral && (
                    <div className="message warning">
                      Insufficient {collateralSymbol} for collateral
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
                  <span className="preview-label">You Will Borrow</span>
                  <span className="preview-value">
                    {buildResponse.summary.amount_out_estimate || buildResponse.summary.amount_in}
                  </span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>Borrow Amount</span>
                    <span>{buildResponse.summary.amount_out_estimate}</span>
                  </div>
                  <div className="detail-row">
                    <span>Collateral Locked</span>
                    <span>{buildResponse.summary.amount_in}</span>
                  </div>
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
                  The Duckpools bot will process your borrow request and send {pool.symbol} to your wallet.
                  Your collateral will be locked until you repay. If not processed by block{' '}
                  {buildResponse.summary.refund_height.toLocaleString()}, you can reclaim your collateral.
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
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" />
                      <path d="M12 17v4" />
                    </svg>
                  </div>
                  <p>Approve in Nautilus</p>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                    Back
                  </button>
                </div>
              )}

              {flow.signMethod === 'mobile' && flow.qrUrl && (
                <div className="qr-signing">
                  <p>Scan with Ergo Mobile Wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG
                      value={flow.qrUrl}
                      size={200}
                      level="M"
                      includeMargin
                      bgColor="white"
                      fgColor="black"
                    />
                  </div>
                  <div className="waiting-spinner" />
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
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
              <p>Your borrow transaction has been submitted to the network.</p>
              <p className="success-note">
                The Duckpools bot will process your request and send {pool.symbol} to your wallet.
              </p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
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

export default BorrowModal
