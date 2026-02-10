/**
 * LendModal Component
 *
 * Modal for depositing assets into Duckpools lending pools.
 * Displays pool info, amount input, fee breakdown, and handles
 * transaction building and signing via Nautilus or ErgoPay.
 */

import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildLendTx,
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
import './LendModal.css'

interface LendModalProps {
  /** Whether the modal is open */
  isOpen: boolean
  /** Callback to close the modal */
  onClose: () => void
  /** The pool to lend to */
  pool: PoolInfo
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

function pollMintStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}

export function LendModal({
  isOpen,
  onClose,
  pool,
  userAddress,
  walletBalance,
  explorerUrl,
  onSuccess,
}: LendModalProps) {
  // Step state
  const [step, setStep] = useState<TxStep>('input')
  const [inputValue, setInputValue] = useState('')
  const [slippageBps, setSlippageBps] = useState(0)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Build response
  const [buildResponse, setBuildResponse] = useState<LendingBuildResponse | null>(null)

  const flow = useTransactionFlow({
    pollStatus: pollMintStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'Lending', operation: 'lend', description: `Lend to ${pool.name} pool` },
  })

  // Get user's available balance for this pool's asset
  const availableBalance = useMemo(() => {
    if (!walletBalance) return 0

    if (pool.is_erg_pool) {
      // For ERG pools, return ERG balance in nanoERG
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

  // Estimate service fee (tier-1 only for real-time display; exact fee comes from build response)
  const estimateServiceFee = (amountRaw: number, isErgPool: boolean): number => {
    const fee = Math.floor(amountRaw / 160)
    if (isErgPool) return Math.max(fee, MIN_BOX_VALUE_NANO)
    return Math.max(fee, 1)
  }

  // Calculate amounts based on input
  const calculated = useMemo(() => {
    const empty = { amount: 0, amountRaw: 0, serviceFee: 0, slippageBuffer: 0, totalToSend: 0, txFee: LENDING_PROXY_FEE_NANO, totalCost: 0, isValid: false, hasEnoughAsset: true, hasEnoughErgForFee: true }
    if (!inputValue) return empty

    const value = parseFloat(inputValue)
    if (isNaN(value) || value <= 0) return empty

    const multiplier = Math.pow(10, pool.decimals)
    const amountRaw = Math.round(value * multiplier)

    // Service fee estimate
    const serviceFee = estimateServiceFee(amountRaw, pool.is_erg_pool)
    const slippageBuffer = Math.floor(amountRaw * slippageBps / 10000)
    const totalToSend = amountRaw + serviceFee + slippageBuffer

    const txFee = LENDING_PROXY_FEE_NANO
    // For ERG pools, total ERG cost includes amount + fee + slippage + tx fee
    const totalCost = pool.is_erg_pool ? totalToSend + txFee : txFee

    // Validation -- check user has enough assets to cover amount + fee + slippage
    const hasEnoughAsset = pool.is_erg_pool
      ? totalToSend <= availableBalance
      : totalToSend <= availableBalance
    const hasEnoughErgForFee = pool.is_erg_pool
      ? (totalToSend + txFee) <= (walletBalance?.erg_nano || 0)
      : txFee <= (walletBalance?.erg_nano || 0)

    return {
      amount: value,
      amountRaw,
      serviceFee,
      slippageBuffer,
      totalToSend,
      txFee,
      totalCost,
      isValid: value > 0 && hasEnoughAsset && hasEnoughErgForFee,
      hasEnoughAsset,
      hasEnoughErgForFee,
    }
  }, [inputValue, pool, availableBalance, walletBalance, slippageBps])

  // Estimate LP tokens to receive (simple estimate based on pool ratio)
  const estimatedLpTokens = useMemo(() => {
    if (!calculated.isValid || calculated.amountRaw <= 0) return '0'

    // Simple estimate: LP tokens = deposit amount * (total LP / total supplied)
    // For a rough estimate, we assume 1:1 ratio for new deposits
    // The actual amount will be determined by the contract
    const totalSupplied = BigInt(pool.total_supplied)
    if (totalSupplied === 0n) {
      // First depositor gets 1:1
      return formatAmount(calculated.amountRaw.toString(), pool.decimals)
    }

    // Estimate based on pool value (this is approximate)
    return formatAmount(calculated.amountRaw.toString(), pool.decimals)
  }, [calculated, pool])

  // Reset modal-specific state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setInputValue('')
      setSlippageBps(0)
      setLoading(false)
      setError(null)
      setBuildResponse(null)
    }
  }, [isOpen])

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

      // Build the lend transaction
      const response = await buildLendTx({
        pool_id: pool.pool_id,
        amount: calculated.amountRaw,
        user_address: userAddress,
        user_utxos: utxos,
        current_height: nodeStatus.chain_height,
        slippage_bps: slippageBps,
      })

      setBuildResponse(response)
      setStep('preview')
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [calculated, pool.pool_id, userAddress, slippageBps])

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
          message: `Lend ${calculated.amount.toFixed(pool.decimals)} ${pool.symbol} to ${pool.name}`,
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
  }, [buildResponse, calculated.amount, pool, flow])

  // Handle max button click -- reserve space for service fee + slippage
  const handleMaxClick = useCallback(() => {
    if (!walletBalance) return

    let maxAmount: number
    if (pool.is_erg_pool) {
      // For ERG pools, leave enough for tx fee + fee overhead + buffer
      const buffer = 10_000_000 // 0.01 ERG buffer
      const available = walletBalance.erg_nano - LENDING_PROXY_FEE_NANO - buffer
      // Solve: amount + fee(amount) + slippage(amount) <= available
      // fee ~= amount/160, slippage = amount*bps/10000
      // amount * (1 + 1/160 + bps/10000) <= available
      const factor = 1 + 1 / 160 + slippageBps / 10000
      maxAmount = Math.max(0, Math.floor(available / factor))
    } else {
      // For token pools, solve same equation on token balance
      const factor = 1 + 1 / 160 + slippageBps / 10000
      maxAmount = Math.max(0, Math.floor(availableBalance / factor))
    }

    const displayAmount = maxAmount / Math.pow(10, pool.decimals)
    setInputValue(displayAmount.toFixed(pool.decimals))
  }, [walletBalance, pool, availableBalance, slippageBps])

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
          <h2>Lend {pool.symbol}</h2>
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
                <div className="pool-info-row">
                  <span className="pool-info-label">Available Liquidity</span>
                  <span className="pool-info-value">
                    {formatAmount(pool.available_liquidity, pool.decimals)} {pool.symbol}
                  </span>
                </div>
              </div>

              {/* Amount Input */}
              <div className="form-group">
                <label className="form-label">Amount to Lend</label>
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

              {/* Slippage Tolerance */}
              <div className="form-group">
                <label className="form-label">Slippage Tolerance</label>
                <div className="slippage-control">
                  <input
                    type="range"
                    min={0}
                    max={200}
                    step={10}
                    value={slippageBps}
                    onChange={(e) => setSlippageBps(Number(e.target.value))}
                    className="slider"
                  />
                  <span className="slippage-value">{(slippageBps / 100).toFixed(1)}%</span>
                </div>
              </div>

              {/* Fee Breakdown */}
              {inputValue && calculated.amount > 0 && (
                <div className="calculated-output">
                  <div className="output-row">
                    <span className="output-label">Expected LP Tokens</span>
                    <span className="output-value">~{estimatedLpTokens}</span>
                  </div>
                  <div className="output-row muted">
                    <span>Protocol Fee</span>
                    <span>~{formatDisplayAmount(calculated.serviceFee / Math.pow(10, pool.decimals))} {pool.symbol}</span>
                  </div>
                  {slippageBps > 0 && (
                    <div className="output-row muted">
                      <span>Slippage Buffer</span>
                      <span>~{formatDisplayAmount(calculated.slippageBuffer / Math.pow(10, pool.decimals))} {pool.symbol}</span>
                    </div>
                  )}
                  <div className="output-row muted">
                    <span>Transaction Fee</span>
                    <span>{formatErg(LENDING_PROXY_FEE_NANO)} ERG</span>
                  </div>
                  <div className="output-row total">
                    <span>Amount to Send</span>
                    <span>{formatDisplayAmount(calculated.totalToSend / Math.pow(10, pool.decimals))} {pool.symbol}</span>
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
                  <span className="preview-label">You Will Deposit</span>
                  <span className="preview-value">
                    {buildResponse.summary.amount_in}
                  </span>
                </div>

                <div className="preview-details">
                  <div className="detail-row">
                    <span>Deposit Amount</span>
                    <span>{buildResponse.summary.amount_in}</span>
                  </div>
                  {buildResponse.summary.service_fee && (
                    <div className="detail-row">
                      <span>Protocol Fee</span>
                      <span>{buildResponse.summary.service_fee}</span>
                    </div>
                  )}
                  {buildResponse.summary.total_to_send && (
                    <div className="detail-row highlight">
                      <span>Amount to Send</span>
                      <span>{buildResponse.summary.total_to_send}</span>
                    </div>
                  )}
                  {buildResponse.summary.amount_out_estimate && (
                    <div className="detail-row">
                      <span>Expected LP Tokens</span>
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
                  The Duckpools bot will process your deposit and send LP tokens to your wallet.
                  If not processed by block {buildResponse.summary.refund_height.toLocaleString()}, you can reclaim your funds.
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
              <p>Your lend transaction has been submitted to the network.</p>
              <p className="success-note">
                The Duckpools bot will process your deposit and send LP tokens to your wallet.
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

export default LendModal
