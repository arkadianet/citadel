import { useState, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { AdvancedOptions, useRecipientAddress } from './AdvancedOptions'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { TX_FEE_NANO } from '../constants'
import type { TxStatusResponse } from '../api/types'
import '../components/DexyMintModal.css'

export type SigmaUsdAction = 'mint_sigusd' | 'redeem_sigusd' | 'mint_sigrsv' | 'redeem_sigrsv'

interface SigmaUsdState {
  sigusd_price_nano: number
  sigrsv_price_nano: number
  max_sigusd_mintable: number
  max_sigrsv_mintable: number
  max_sigrsv_redeemable: number
  can_mint_sigusd: boolean
  can_mint_sigrsv: boolean
  can_redeem_sigusd: boolean
  can_redeem_sigrsv: boolean
}

interface TransactionModalProps {
  isOpen: boolean
  onClose: () => void
  action: SigmaUsdAction
  walletAddress: string
  ergBalance: number
  tokenBalance?: number
  explorerUrl: string
  onSuccess: (txId: string) => void
  state: SigmaUsdState
}

type TxStep = 'input' | 'signing' | 'success' | 'error'

const PROTOCOL_FEE_RATE = 0.02

const ACTION_CONFIG = {
  mint_sigusd: {
    title: 'Mint SigUSD',
    decimals: 2,
    isRedeem: false,
    tokenName: 'SigUSD',
    icon: '/icons/sigmausd.svg',
  },
  redeem_sigusd: {
    title: 'Redeem SigUSD',
    decimals: 2,
    isRedeem: true,
    tokenName: 'SigUSD',
    icon: '/icons/sigmausd.svg',
  },
  mint_sigrsv: {
    title: 'Mint SigRSV',
    decimals: 0,
    isRedeem: false,
    tokenName: 'SigRSV',
    icon: '/icons/sigrsv.svg',
  },
  redeem_sigrsv: {
    title: 'Redeem SigRSV',
    decimals: 0,
    isRedeem: true,
    tokenName: 'SigRSV',
    icon: '/icons/sigrsv.svg',
  },
}

function pollMintStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
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
  state,
}: TransactionModalProps) {
  const config = ACTION_CONFIG[action]
  const { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } = useRecipientAddress()
  const [step, setStep] = useState<TxStep>('input')
  const [ergInput, setErgInput] = useState('')
  const [tokenInput, setTokenInput] = useState('')
  const [lastEdited, setLastEdited] = useState<'erg' | 'token'>('token')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: pollMintStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'SigmaUSD', operation: config.isRedeem ? 'redeem' : 'mint', description: `${config.title}` },
  })

  // Token price in nanoERG per 1 display unit
  const priceNano = useMemo(() => {
    return action.includes('sigusd') ? state.sigusd_price_nano : state.sigrsv_price_nano
  }, [action, state.sigusd_price_nano, state.sigrsv_price_nano])

  // Price in ERG per display unit (for input calculations)
  const priceErg = priceNano / 1e9

  // Calculate amounts in real-time
  const calculated = useMemo(() => {
    const activeInput = lastEdited === 'erg' ? ergInput : tokenInput
    if (!activeInput || !priceErg) {
      return { tokenAmount: 0, ergAmount: 0, protocolFee: 0, totalErg: 0, isValid: false, hasEnoughErg: true, hasEnoughTokens: true, withinLimit: true }
    }

    const value = parseFloat(activeInput)
    if (isNaN(value) || value <= 0) {
      return { tokenAmount: 0, ergAmount: 0, protocolFee: 0, totalErg: 0, isValid: false, hasEnoughErg: true, hasEnoughTokens: true, withinLimit: true }
    }

    const tokenMultiplier = Math.pow(10, config.decimals)
    const txFeeErg = TX_FEE_NANO / 1e9
    let tokenAmount: number
    let ergBase: number

    if (config.isRedeem) {
      // Redeem: user provides tokens, receives ERG
      if (lastEdited === 'token') {
        tokenAmount = value
        ergBase = tokenAmount * priceErg
      } else {
        // User typed desired ERG output → calculate token amount
        ergBase = (value + txFeeErg) / (1 - PROTOCOL_FEE_RATE)
        tokenAmount = ergBase / priceErg
        tokenAmount = Math.ceil(tokenAmount * tokenMultiplier) / tokenMultiplier
        ergBase = tokenAmount * priceErg
      }
      const protocolFee = ergBase * PROTOCOL_FEE_RATE
      const netErg = ergBase - protocolFee - txFeeErg

      const tokenAmountRaw = Math.round(tokenAmount * tokenMultiplier)
      const hasEnoughTokens = tokenBalance !== undefined ? tokenAmountRaw <= tokenBalance : true
      const hasEnoughErg = true // Redeem doesn't need ERG (only tx fee, covered by proceeds)

      // Check limits
      let withinLimit = true
      if (action === 'redeem_sigrsv' && state.max_sigrsv_redeemable > 0) {
        withinLimit = tokenAmountRaw <= state.max_sigrsv_redeemable
      }

      return {
        tokenAmount,
        tokenAmountRaw,
        ergAmount: ergBase,
        protocolFee,
        netErg,
        totalErg: txFeeErg,
        isValid: tokenAmount > 0 && netErg > 0 && hasEnoughTokens && withinLimit,
        hasEnoughErg,
        hasEnoughTokens,
        withinLimit,
      }
    } else {
      // Mint: user pays ERG, receives tokens
      if (lastEdited === 'token') {
        tokenAmount = value
        ergBase = tokenAmount * priceErg
      } else {
        // User typed ERG amount → calculate tokens
        ergBase = (value - txFeeErg) / (1 + PROTOCOL_FEE_RATE)
        tokenAmount = ergBase / priceErg
        tokenAmount = Math.floor(tokenAmount * tokenMultiplier) / tokenMultiplier
        ergBase = tokenAmount * priceErg
      }
      const protocolFee = ergBase * PROTOCOL_FEE_RATE
      const totalErg = ergBase + protocolFee + txFeeErg

      const tokenAmountRaw = Math.round(tokenAmount * tokenMultiplier)
      const hasEnoughErg = totalErg <= ergBalance / 1e9
      const hasEnoughTokens = true

      // Check limits
      let withinLimit = true
      if (action === 'mint_sigusd' && state.max_sigusd_mintable > 0) {
        withinLimit = tokenAmountRaw <= state.max_sigusd_mintable
      }
      if (action === 'mint_sigrsv' && state.max_sigrsv_mintable > 0) {
        withinLimit = tokenAmountRaw <= state.max_sigrsv_mintable
      }

      return {
        tokenAmount,
        tokenAmountRaw,
        ergAmount: ergBase,
        protocolFee,
        totalErg,
        isValid: tokenAmount > 0 && hasEnoughErg && withinLimit,
        hasEnoughErg,
        hasEnoughTokens,
        withinLimit,
      }
    }
  }, [ergInput, tokenInput, lastEdited, priceErg, config.decimals, config.isRedeem, ergBalance, tokenBalance, action, state])

  useEffect(() => {
    if (isOpen) {
      setStep('input')
      setErgInput('')
      setTokenInput('')
      setLastEdited('token')
      setError(null)
      setRecipientAddress('')
    }
  }, [isOpen, action])

  const handleErgChange = (value: string) => {
    setErgInput(value)
    setLastEdited('erg')
    setError(null)
    const parsed = parseFloat(value)
    if (!isNaN(parsed) && parsed > 0 && priceErg) {
      const txFeeErg = TX_FEE_NANO / 1e9
      const tokenMultiplier = Math.pow(10, config.decimals)
      let tokenValue: number
      if (config.isRedeem) {
        const ergBase = (parsed + txFeeErg) / (1 - PROTOCOL_FEE_RATE)
        tokenValue = ergBase / priceErg
        tokenValue = Math.ceil(tokenValue * tokenMultiplier) / tokenMultiplier
      } else {
        const ergBase = (parsed - txFeeErg) / (1 + PROTOCOL_FEE_RATE)
        tokenValue = ergBase / priceErg
        tokenValue = Math.floor(tokenValue * tokenMultiplier) / tokenMultiplier
      }
      if (tokenValue > 0) {
        setTokenInput(config.decimals === 0 ? Math.floor(tokenValue).toString() : tokenValue.toFixed(config.decimals))
      } else {
        setTokenInput('')
      }
    } else {
      setTokenInput('')
    }
  }

  const handleTokenChange = (value: string) => {
    setTokenInput(value)
    setLastEdited('token')
    setError(null)
    const parsed = parseFloat(value)
    if (!isNaN(parsed) && parsed > 0 && priceErg) {
      const txFeeErg = TX_FEE_NANO / 1e9
      const ergBase = parsed * priceErg
      if (config.isRedeem) {
        const protocolFee = ergBase * PROTOCOL_FEE_RATE
        const netErg = ergBase - protocolFee - txFeeErg
        setErgInput(netErg > 0 ? netErg.toFixed(4) : '')
      } else {
        const protocolFee = ergBase * PROTOCOL_FEE_RATE
        const totalErg = ergBase + protocolFee + txFeeErg
        setErgInput(totalErg.toFixed(4))
      }
    } else {
      setErgInput('')
    }
  }

  const handleMaxClick = () => {
    if (!priceErg) return
    const txFeeErg = TX_FEE_NANO / 1e9
    const tokenMultiplier = Math.pow(10, config.decimals)

    if (config.isRedeem) {
      // Max is the token balance (or limit, whichever is lower)
      let maxRaw = tokenBalance ?? 0
      if (action === 'redeem_sigrsv' && state.max_sigrsv_redeemable > 0) {
        maxRaw = Math.min(maxRaw, state.max_sigrsv_redeemable)
      }
      const maxToken = maxRaw / tokenMultiplier
      const ergBase = maxToken * priceErg
      const protocolFee = ergBase * PROTOCOL_FEE_RATE
      const netErg = ergBase - protocolFee - txFeeErg

      setTokenInput(config.decimals === 0 ? Math.floor(maxToken).toString() : maxToken.toFixed(config.decimals))
      setErgInput(netErg > 0 ? netErg.toFixed(4) : '0')
      setLastEdited('token')
    } else {
      // Max from ERG balance
      const availableErg = (ergBalance / 1e9) - txFeeErg - 0.001 // small buffer
      const ergBase = availableErg / (1 + PROTOCOL_FEE_RATE)
      let maxToken = ergBase / priceErg
      maxToken = Math.floor(maxToken * tokenMultiplier) / tokenMultiplier

      // Respect minting limits
      if (action === 'mint_sigusd' && state.max_sigusd_mintable > 0) {
        maxToken = Math.min(maxToken, state.max_sigusd_mintable / tokenMultiplier)
      }
      if (action === 'mint_sigrsv' && state.max_sigrsv_mintable > 0) {
        maxToken = Math.min(maxToken, state.max_sigrsv_mintable / tokenMultiplier)
      }

      const ergForMax = maxToken * priceErg
      const fee = ergForMax * PROTOCOL_FEE_RATE
      const total = ergForMax + fee + txFeeErg

      setTokenInput(config.decimals === 0 ? Math.floor(maxToken).toString() : maxToken.toFixed(config.decimals))
      setErgInput(total.toFixed(4))
      setLastEdited('token')
    }
  }

  const handleSign = async () => {
    if (!calculated.isValid || !calculated.tokenAmountRaw) return

    setLoading(true)
    setError(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')

      const buildResult = await invoke<{ unsigned_tx: object; summary: object }>('build_sigmausd_tx', {
        request: {
          action,
          amount: calculated.tokenAmountRaw,
          user_address: walletAddress,
          user_utxos: utxos,
          current_height: nodeStatus.chain_height,
          recipient_address: recipientOrNull,
        }
      })

      const signResult = await invoke<{ request_id: string; ergopay_url: string; nautilus_url: string }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `${config.title}: ${tokenInput} ${config.tokenName}`
        }
      })

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  if (!isOpen) return null

  const showInfo = (ergInput || tokenInput) && calculated.tokenAmount > 0

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal dexy-mint-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{config.title}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'input' && (
            <div className="mint-input-step">
              {/* Swap Inputs */}
              <div className="swap-inputs">
                {/* Top field: You Pay (ERG for mint, Token for redeem) */}
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">
                      {config.isRedeem ? 'You Provide' : 'You Pay'}
                    </span>
                    <span className="swap-field-balance">
                      {config.isRedeem
                        ? `Balance: ${tokenBalance !== undefined ? (tokenBalance / Math.pow(10, config.decimals)).toLocaleString(undefined, { maximumFractionDigits: config.decimals }) : '0'} ${config.tokenName}`
                        : `Balance: ${formatErg(ergBalance)} ERG`
                      }
                    </span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={config.isRedeem ? tokenInput : ergInput}
                      onChange={e => config.isRedeem ? handleTokenChange(e.target.value) : handleErgChange(e.target.value)}
                      placeholder="0"
                      min="0"
                      step={config.isRedeem ? Math.pow(10, -config.decimals) : 0.0001}
                    />
                    <div className="swap-field-token">
                      <span>{config.isRedeem ? config.tokenName : 'ERG'}</span>
                      <button className="max-btn" onClick={handleMaxClick} type="button">MAX</button>
                    </div>
                  </div>
                </div>

                {/* Arrow separator */}
                <div className="swap-arrow">&darr;</div>

                {/* Bottom field: You Receive (Token for mint, ERG for redeem) */}
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Receive</span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={config.isRedeem ? ergInput : tokenInput}
                      onChange={e => config.isRedeem ? handleErgChange(e.target.value) : handleTokenChange(e.target.value)}
                      placeholder="0"
                      min="0"
                      step={config.isRedeem ? 0.0001 : Math.pow(10, -config.decimals)}
                    />
                    <span className="swap-field-token">
                      {config.isRedeem ? 'ERG' : config.tokenName}
                    </span>
                  </div>
                </div>
              </div>

              {/* Validation Warnings */}
              {showInfo && !calculated.hasEnoughErg && (
                <div className="message warning">
                  Insufficient ERG balance (need {calculated.totalErg.toFixed(4)} ERG)
                </div>
              )}
              {showInfo && !calculated.hasEnoughTokens && (
                <div className="message warning">
                  Insufficient {config.tokenName} balance
                </div>
              )}
              {showInfo && !calculated.withinLimit && (
                <div className="message warning">
                  Exceeds {config.isRedeem ? 'redeemable' : 'mintable'} limit
                </div>
              )}

              {/* Inline Fee Breakdown */}
              {showInfo && (
                <div className="mint-info">
                  <div className="info-row">
                    <span>Rate</span>
                    <span>{priceErg.toFixed(config.tokenName === 'SigRSV' ? 8 : 4)} ERG / {config.tokenName}</span>
                  </div>
                  <div className="info-row">
                    <span>Protocol Fee (2%)</span>
                    <span>{calculated.protocolFee.toFixed(4)} ERG</span>
                  </div>
                  <div className="info-row">
                    <span>Transaction Fee</span>
                    <span>{(TX_FEE_NANO / 1e9).toFixed(4)} ERG</span>
                  </div>
                </div>
              )}

              <AdvancedOptions
                recipientAddress={recipientAddress}
                onRecipientChange={setRecipientAddress}
                addressValid={addressValid}
              />

              {error && <div className="message error">{error}</div>}

              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleSign}
                  disabled={loading || !calculated.isValid || (!!recipientAddress && addressValid !== true)}
                >
                  {loading ? 'Building...' : 'Sign Transaction'}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && (
            <div className="mint-signing-step">
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
            <div className="mint-success-step">
              <div className="success-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--success)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Transaction Submitted!</h3>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { if (flow.txId) onSuccess(flow.txId); onClose(); }}>
                Done
              </button>
            </div>
          )}

          {step === 'error' && (
            <div className="mint-error-step">
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
                <button className="btn btn-primary" onClick={() => { setStep('input'); setError(null) }}>
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
