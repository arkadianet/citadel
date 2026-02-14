/**
 * BorrowModal Component
 *
 * Modal for borrowing assets from Duckpools lending pools.
 * Matches the Duckpools UX: user enters borrow amount, selects a collateral
 * loan ratio (150%/170%/200%/Custom), and collateral is auto-calculated
 * from the DEX price oracle.
 *
 * Follows the same step-based pattern as LendModal:
 * input -> preview -> signing -> success/error
 */

import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildBorrowTx,
  getDexPrice,
  formatAmount,
  formatApy,
  type PoolInfo,
  type CollateralOption,
  type DexPriceInfo,
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
  const [selectedCollateralIdx, setSelectedCollateralIdx] = useState(0)
  const [collateralRatio, setCollateralRatio] = useState(170)
  const [customRatio, setCustomRatio] = useState('')
  const [isCustomRatio, setIsCustomRatio] = useState(false)
  const [dexPrice, setDexPrice] = useState<DexPriceInfo | null>(null)
  const [priceLoading, setPriceLoading] = useState(false)
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

  // Collateral options from on-chain parameter box
  const hasCollateralOption = pool.collateral_options.length > 0
  const collateralOption: CollateralOption | null = pool.collateral_options[selectedCollateralIdx] ?? null

  // For token pools, collateral is ERG ("native"); for ERG pool, it's a specific token
  const collateralIsErg = collateralOption?.token_id === 'native'
  const collateralSymbol = collateralOption?.token_name ?? (collateralIsErg ? 'ERG' : 'Token')

  // Derive collateral decimals from known token configs (not wallet balance,
  // which would return 0 if user doesn't hold the token yet)
  const collateralDecimals = useMemo(() => {
    if (collateralIsErg) return 9
    if (!collateralOption) return 0
    const KNOWN_DECIMALS: Record<string, number> = {
      '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04': 2,  // SigUSD
      '003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0': 0,  // SigRSV
      '8b08cdd5449a9592a9e79711d7d79249d7a03c535d17efaee83e216e80a44c4b': 4,  // RSN
      'e023c5f382b6e96fbd878f6811aac73345489032157ad5affb84aefd4956c297': 6,  // rsADA
      '9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922269913e0de369d': 6,  // SPF
      '7a51950e5f548549ec1aa63ffdc38279505b11e7e803d01bcf8347e0123c88b0': 8,  // rsBTC
      '089990451bb430f05a85f4ef3bcb6ebf852b3d6ee68d86d78658b9ccef20074f': 0,  // QUACKS
    }
    return KNOWN_DECIMALS[collateralOption.token_id] ?? walletBalance?.tokens.find(t => t.token_id === collateralOption.token_id)?.decimals ?? 0
  }, [collateralIsErg, collateralOption, walletBalance])

  // Liquidation threshold as percentage (e.g. 1400 -> 140)
  const liquidationPct = collateralOption ? collateralOption.liquidation_threshold / 10 : 0
  const liquidationPenaltyPct = collateralOption ? collateralOption.liquidation_penalty / 10 : 0

  // Minimum ratio = liquidation threshold + 10% buffer
  const minRatio = Math.ceil(liquidationPct) + 10

  // Ratio presets derived from min
  const ratioPresets = useMemo(() => [
    minRatio,
    minRatio + 20,
    minRatio + 50,
  ], [minRatio])

  // Active ratio (from presets or custom)
  const activeRatio = isCustomRatio ? (parseInt(customRatio) || minRatio) : collateralRatio

  // Fetch DEX price when modal opens or collateral selection changes
  useEffect(() => {
    if (!isOpen || !collateralOption?.dex_nft) {
      setDexPrice(null)
      return
    }
    let cancelled = false
    setPriceLoading(true)
    getDexPrice(collateralOption.dex_nft)
      .then((price) => { if (!cancelled) setDexPrice(price) })
      .catch((e) => { if (!cancelled) console.error('Failed to fetch DEX price:', e) })
      .finally(() => { if (!cancelled) setPriceLoading(false) })
    return () => { cancelled = true }
  }, [isOpen, collateralOption?.dex_nft])

  // Auto-calculate collateral from borrow amount, ratio, and DEX price
  const calculated = useMemo(() => {
    const empty = {
      borrowAmount: 0,
      borrowAmountRaw: 0,
      collateralAmount: 0,
      collateralAmountRaw: 0,
      collateralDisplay: '',
      txFee: LENDING_PROXY_FEE_NANO,
      isValid: false,
      hasEnoughCollateral: true,
      hasEnoughErgForFee: true,
      borrowExceedsLiquidity: false,
      ratioTooLow: false,
    }

    if (!borrowInputValue || !dexPrice || !collateralOption) return empty

    const borrowVal = parseFloat(borrowInputValue)
    if (isNaN(borrowVal) || borrowVal <= 0) return empty

    const borrowMultiplier = Math.pow(10, pool.decimals)
    const borrowAmountRaw = Math.round(borrowVal * borrowMultiplier)

    // Validate ratio
    const ratioTooLow = activeRatio < minRatio

    // Calculate collateral using DEX price (raw unit ratios from backend)
    // Backend returns: erg_per_token = nanoERG / raw_token_unit
    //                  token_per_erg = raw_token_units / nanoERG
    // So we use borrowAmountRaw (raw units) to get collateral in raw units directly.
    let collateralAmountRaw: number
    let collateralDisplay: string

    if (collateralIsErg) {
      // Token pool: borrow tokens, collateral is ERG (nanoERG)
      // borrowAmountRaw is in raw token units, erg_per_token is nanoERG per raw token unit
      collateralAmountRaw = Math.ceil(borrowAmountRaw * dexPrice.erg_per_token * activeRatio / 100)
      collateralDisplay = `${(collateralAmountRaw / 1e9).toFixed(4)} ERG`
    } else {
      // ERG pool: borrow ERG, collateral is token (raw token units)
      // borrowAmountRaw is in nanoERG, token_per_erg is raw token units per nanoERG
      collateralAmountRaw = Math.ceil(borrowAmountRaw * dexPrice.token_per_erg * activeRatio / 100)
      const collateralMultiplier = Math.pow(10, collateralDecimals)
      collateralDisplay = `${(collateralAmountRaw / collateralMultiplier).toFixed(Math.min(collateralDecimals, 4))} ${collateralSymbol}`
    }

    const collateralAmount = collateralIsErg
      ? collateralAmountRaw / 1e9
      : collateralAmountRaw / Math.pow(10, collateralDecimals)

    // Validation
    const borrowExceedsLiquidity = BigInt(borrowAmountRaw) > BigInt(pool.available_liquidity)

    let hasEnoughCollateral = true
    let hasEnoughErgForFee = true

    if (collateralIsErg) {
      const totalErgNeeded = collateralAmountRaw + MIN_BOX_VALUE_NANO + LENDING_PROXY_FEE_NANO * 2 + MIN_BOX_VALUE_NANO
      hasEnoughCollateral = totalErgNeeded <= (walletBalance?.erg_nano || 0)
      hasEnoughErgForFee = hasEnoughCollateral
    } else {
      const totalErgNeeded = MIN_BOX_VALUE_NANO + LENDING_PROXY_FEE_NANO * 2 + MIN_BOX_VALUE_NANO
      hasEnoughErgForFee = totalErgNeeded <= (walletBalance?.erg_nano || 0)
      const walletToken = walletBalance?.tokens.find(t => t.token_id === collateralOption.token_id)
      hasEnoughCollateral = collateralAmountRaw <= (walletToken?.amount || 0)
    }

    return {
      borrowAmount: borrowVal,
      borrowAmountRaw,
      collateralAmount,
      collateralAmountRaw,
      collateralDisplay,
      txFee: LENDING_PROXY_FEE_NANO,
      isValid: borrowVal > 0 && collateralAmountRaw > 0 && hasEnoughCollateral && hasEnoughErgForFee && !borrowExceedsLiquidity && !ratioTooLow,
      hasEnoughCollateral,
      hasEnoughErgForFee,
      borrowExceedsLiquidity,
      ratioTooLow,
    }
  }, [borrowInputValue, dexPrice, pool, collateralOption, collateralIsErg, collateralDecimals, collateralSymbol, activeRatio, minRatio, walletBalance])

  // Reset state when modal opens
  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setBorrowInputValue('')
      setSelectedCollateralIdx(0)
      setIsCustomRatio(false)
      setCustomRatio('')
      setLoading(false)
      setError(null)
      setBuildResponse(null)
    }
  }, [isOpen])

  // Set default ratio based on presets when they change
  useEffect(() => {
    if (ratioPresets.length >= 2) {
      setCollateralRatio(ratioPresets[1]) // Default to middle preset (e.g. 170%)
    }
  }, [ratioPresets])

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

      const collateralToken = collateralOption?.token_id || 'native'

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
  }, [calculated, pool.pool_id, userAddress, collateralOption])

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
          <span className="modal-header-detail">{formatApy(pool.borrow_apy)} Interest Rate</span>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="lend-input-step">
              {/* Borrow Amount Input (primary) */}
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

              {/* Collateral Type Selector (ERG pool with multiple options) */}
              {pool.collateral_options.length > 1 && (
                <div className="form-group">
                  <label className="form-label">Collateral Token</label>
                  <select
                    className="input"
                    value={selectedCollateralIdx}
                    onChange={(e) => {
                      setSelectedCollateralIdx(Number(e.target.value))
                    }}
                  >
                    {pool.collateral_options.map((opt, idx) => (
                      <option key={opt.token_id} value={idx}>
                        {opt.token_name}
                      </option>
                    ))}
                  </select>
                </div>
              )}

              {/* Collateral Loan Ratio */}
              <div className="form-group">
                <label className="form-label">Collateral Loan Ratio</label>
                <div className="ratio-selector">
                  {ratioPresets.map((preset) => (
                    <button
                      key={preset}
                      className={`ratio-btn ${!isCustomRatio && collateralRatio === preset ? 'active' : ''}`}
                      onClick={() => {
                        setCollateralRatio(preset)
                        setIsCustomRatio(false)
                      }}
                    >
                      {preset}%
                    </button>
                  ))}
                  <button
                    className={`ratio-btn ${isCustomRatio ? 'active' : ''}`}
                    onClick={() => setIsCustomRatio(true)}
                  >
                    Custom
                  </button>
                </div>
                {isCustomRatio && (
                  <div className="input-with-max" style={{ marginTop: '0.5rem' }}>
                    <input
                      type="number"
                      className="input"
                      value={customRatio}
                      onChange={(e) => setCustomRatio(e.target.value)}
                      placeholder={`Min ${minRatio}%`}
                      min={minRatio}
                    />
                    <div className="input-suffix">
                      <span className="input-currency">%</span>
                    </div>
                  </div>
                )}
              </div>

              {/* Collateral to Pledge (auto-calculated, read-only) */}
              <div className="form-group">
                <label className="form-label">Collateral to Pledge</label>
                <div className="calculated-collateral">
                  {priceLoading ? (
                    <span className="muted">Loading price...</span>
                  ) : !dexPrice ? (
                    <span className="muted">Price unavailable</span>
                  ) : borrowInputValue && calculated.collateralAmountRaw > 0 ? (
                    <span className="collateral-value">{calculated.collateralDisplay}</span>
                  ) : (
                    <span className="muted">Enter borrow amount</span>
                  )}
                </div>
              </div>

              {/* Loan Terms Info */}
              {collateralOption && (
                <div className="pool-info-card">
                  <div className="pool-info-row">
                    <span className="pool-info-label">Liquidation Threshold</span>
                    <span className="pool-info-value">{liquidationPct.toFixed(0)}%</span>
                  </div>
                  {liquidationPenaltyPct > 0 && (
                    <div className="pool-info-row">
                      <span className="pool-info-label">Liquidation Penalty</span>
                      <span className="pool-info-value">{liquidationPenaltyPct.toFixed(0)}%</span>
                    </div>
                  )}
                  <div className="pool-info-row">
                    <span className="pool-info-label">Transaction Fee</span>
                    <span className="pool-info-value">{formatErg(LENDING_PROXY_FEE_NANO)} ERG</span>
                  </div>
                  <div className="pool-info-row">
                    <span className="pool-info-label">Refund Available</span>
                    <span className="pool-info-value">~24h (720 blocks)</span>
                  </div>
                </div>
              )}

              {/* Validation Warnings */}
              {borrowInputValue && calculated.borrowAmount > 0 && (
                <>
                  {calculated.borrowExceedsLiquidity && (
                    <div className="message warning">
                      Borrow amount exceeds available pool liquidity
                    </div>
                  )}
                  {calculated.ratioTooLow && (
                    <div className="message warning">
                      Ratio must be at least {minRatio}% (liquidation at {liquidationPct.toFixed(0)}%)
                    </div>
                  )}
                  {!calculated.hasEnoughCollateral && !calculated.ratioTooLow && (
                    <div className="message warning">
                      Insufficient {collateralSymbol} for collateral ({calculated.collateralDisplay} required)
                    </div>
                  )}
                  {!calculated.hasEnoughErgForFee && calculated.hasEnoughCollateral && (
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
                  disabled={loading || !calculated.isValid || priceLoading}
                >
                  {loading ? 'Building...' : `Borrow ${pool.symbol}`}
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
                    <span>{calculated.collateralDisplay}</span>
                  </div>
                  <div className="detail-row">
                    <span>Collateral Ratio</span>
                    <span>{activeRatio}%</span>
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
                  Your collateral will be locked until you repay. Liquidation occurs if collateral
                  value drops below {liquidationPct.toFixed(0)}% of loan value.
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
