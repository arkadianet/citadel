import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildCancelOrder,
  buildCloseOrder,
  buildRepay,
  buildLiquidate,
  startSigmaFiSign,
  getSigmaFiTxStatus,
  formatAmount,
  formatPercent,
  blocksToTimeString,
  truncateAddress,
  type OpenOrder,
  type ActiveBond,
} from '../api/sigmafi'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'
import './SigmaFiConfirmModal.css'

export type ConfirmMode = 'cancel' | 'lend' | 'repay' | 'liquidate'

interface SigmaFiConfirmModalProps {
  isOpen: boolean
  onClose: () => void
  onSuccess: () => void
  walletAddress: string
  explorerUrl: string
  mode: ConfirmMode
  order?: OpenOrder
  bond?: ActiveBond
}

type ModalStep = 'confirm' | 'signing' | 'success' | 'error'

/** Our UI fee ErgoTree — same as DEV_FEE_ERGO_TREE from SigmaFi constants */
const UI_FEE_ERGO_TREE =
  '0008cd03a11d3028b9bc57b6ac724485e99960b89c278db6bab5d2b961b01aee29405a02'

function pollSigmaFiStatus(requestId: string): Promise<TxStatusResponse> {
  return getSigmaFiTxStatus(requestId)
}

const MODE_CONFIG = {
  cancel: { title: 'Cancel Order', verb: 'Cancel', btnClass: 'danger' as const },
  lend: { title: 'Fill Loan Request', verb: 'Lend', btnClass: 'primary' as const },
  repay: { title: 'Repay Bond', verb: 'Repay', btnClass: 'primary' as const },
  liquidate: { title: 'Liquidate Bond', verb: 'Liquidate', btnClass: 'danger' as const },
}

export function SigmaFiConfirmModal({
  isOpen,
  onClose,
  onSuccess,
  explorerUrl,
  mode,
  order,
  bond,
}: SigmaFiConfirmModalProps) {
  const [step, setStep] = useState<ModalStep>('confirm')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const config = MODE_CONFIG[mode]

  const flow = useTransactionFlow({
    pollStatus: pollSigmaFiStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'SigmaFi', operation: mode, description: `SigmaFi ${mode}` },
  })

  useEffect(() => {
    if (isOpen) {
      setStep('confirm')
      setLoading(false)
      setError(null)
    }
  }, [isOpen])

  const handleConfirm = useCallback(async () => {
    setLoading(true)
    setError(null)

    try {
      // Get UTXOs and height
      const utxos = await invoke<object[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const userErgoTree = (utxos[0] as { ergoTree: string }).ergoTree

      let unsignedTx: object
      let message: string

      switch (mode) {
        case 'cancel': {
          if (!order) throw new Error('No order provided')
          unsignedTx = await buildCancelOrder(
            order.boxId,
            userErgoTree,
            utxos,
            nodeStatus.chain_height,
          )
          message = `Cancel SigmaFi order`
          break
        }
        case 'lend': {
          if (!order) throw new Error('No order provided')
          unsignedTx = await buildCloseOrder(
            order.boxId,
            userErgoTree,
            UI_FEE_ERGO_TREE,
            order.loanTokenId,
            utxos,
            nodeStatus.chain_height,
          )
          message = `Lend ${formatAmount(order.principal, order.loanTokenDecimals)} ${order.loanTokenName}`
          break
        }
        case 'repay': {
          if (!bond) throw new Error('No bond provided')
          unsignedTx = await buildRepay(
            bond.boxId,
            bond.loanTokenId,
            userErgoTree,
            utxos,
            nodeStatus.chain_height,
          )
          message = `Repay ${formatAmount(bond.repayment, bond.loanTokenDecimals)} ${bond.loanTokenName}`
          break
        }
        case 'liquidate': {
          if (!bond) throw new Error('No bond provided')
          unsignedTx = await buildLiquidate(
            bond.boxId,
            userErgoTree,
            utxos,
            nodeStatus.chain_height,
          )
          message = `Liquidate bond collateral`
          break
        }
      }

      const signResult = await startSigmaFiSign(unsignedTx, message)
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }, [mode, order, bond, flow])

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal sigmafi-confirm-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{config.title}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {step === 'confirm' && (
            <div className="sf-confirm-step">
              {/* Order details for cancel/lend */}
              {(mode === 'cancel' || mode === 'lend') && order && (
                <div className="sf-details-card">
                  <div className="sf-details-header">
                    <span className="sigmafi-token-badge">{order.loanTokenName}</span>
                    {order.isOwn && <span className="sigmafi-own-badge">Your Order</span>}
                  </div>
                  <div className="sf-detail-row">
                    <span>Principal</span>
                    <span>{formatAmount(order.principal, order.loanTokenDecimals)} {order.loanTokenName}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Repayment</span>
                    <span>{formatAmount(order.repayment, order.loanTokenDecimals)} {order.loanTokenName}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Interest</span>
                    <span className="highlight">{formatPercent(order.interestPercent)}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Term</span>
                    <span>{blocksToTimeString(order.maturityBlocks)}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Collateral</span>
                    <span>{formatAmount(order.collateralErg, 9)} ERG</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Borrower</span>
                    <span className="mono">{truncateAddress(order.borrowerAddress)}</span>
                  </div>

                  {mode === 'lend' && (
                    <div className="sf-fee-breakdown">
                      <div className="sf-detail-row muted">
                        <span>Dev Fee (0.5%)</span>
                        <span>{formatAmount(Math.floor(order.principal * 0.005), order.loanTokenDecimals)} {order.loanTokenName}</span>
                      </div>
                      <div className="sf-detail-row muted">
                        <span>UI Fee (0.4%)</span>
                        <span>{formatAmount(Math.floor(order.principal * 0.004), order.loanTokenDecimals)} {order.loanTokenName}</span>
                      </div>
                    </div>
                  )}

                  {mode === 'cancel' && (
                    <div className="sf-notice">
                      Your collateral will be returned to your wallet.
                    </div>
                  )}
                </div>
              )}

              {/* Bond details for repay/liquidate */}
              {(mode === 'repay' || mode === 'liquidate') && bond && (
                <div className="sf-details-card">
                  <div className="sf-details-header">
                    <span className="sigmafi-token-badge">{bond.loanTokenName}</span>
                    {bond.blocksRemaining <= 0
                      ? <span className="sigmafi-status-badge danger">Past Due</span>
                      : <span className="sigmafi-status-badge active">Active</span>
                    }
                  </div>
                  <div className="sf-detail-row">
                    <span>Repayment Amount</span>
                    <span>{formatAmount(bond.repayment, bond.loanTokenDecimals)} {bond.loanTokenName}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Collateral</span>
                    <span>{formatAmount(bond.collateralErg, 9)} ERG</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Maturity Height</span>
                    <span className="mono">{bond.maturityHeight.toLocaleString()}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Time Remaining</span>
                    <span className={bond.blocksRemaining <= 0 ? 'danger' : ''}>
                      {bond.blocksRemaining <= 0
                        ? `Overdue ${blocksToTimeString(-bond.blocksRemaining)}`
                        : blocksToTimeString(bond.blocksRemaining)}
                    </span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Borrower</span>
                    <span className="mono">{truncateAddress(bond.borrowerAddress)}</span>
                  </div>
                  <div className="sf-detail-row">
                    <span>Lender</span>
                    <span className="mono">{truncateAddress(bond.lenderAddress)}</span>
                  </div>

                  {mode === 'repay' && (
                    <div className="sf-notice success">
                      Repaying will return your collateral ({formatAmount(bond.collateralErg, 9)} ERG) to your wallet.
                    </div>
                  )}

                  {mode === 'liquidate' && (
                    <div className="sf-notice warning">
                      As lender, you will claim the collateral ({formatAmount(bond.collateralErg, 9)} ERG).
                    </div>
                  )}
                </div>
              )}

              {error && <div className="message error">{error}</div>}

              <div className="sf-modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>
                  Cancel
                </button>
                <button
                  className={`btn btn-${config.btnClass}`}
                  onClick={handleConfirm}
                  disabled={loading}
                >
                  {loading ? 'Building...' : config.verb}
                </button>
              </div>
            </div>
          )}

          {step === 'signing' && (
            <div className="sf-signing-step">
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
            <div className="sf-success-step">
              <div className="success-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h3>Transaction Submitted!</h3>
              <p>Your {mode} transaction has been submitted to the network.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { onSuccess(); onClose() }}>
                Done
              </button>
            </div>
          )}

          {step === 'error' && (
            <div className="sf-error-step">
              <div className="error-icon">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--red-400)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="15" y1="9" x2="9" y2="15" />
                  <line x1="9" y1="9" x2="15" y2="15" />
                </svg>
              </div>
              <h3>Transaction Failed</h3>
              <p className="error-message">{error}</p>
              <div className="sf-modal-actions">
                <button className="btn btn-secondary" onClick={onClose}>Close</button>
                <button className="btn btn-primary" onClick={() => setStep('confirm')}>Try Again</button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
