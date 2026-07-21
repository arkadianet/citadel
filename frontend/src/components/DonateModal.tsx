import { useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { buildDonationTx, getTxStatus, startSign } from '../api/donate'
import { DEFAULT_DEV_FEE_ADDRESS, DEV_FEE_NANO, MIN_BOX_VALUE_NANO, WALLET_TX_FEES_NANO } from '../constants'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { formatErg, truncateAddress } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { Button, FormField, Input, Modal, Spinner } from './ui'
import './DonateModal.css'

const PRESETS = [0.1, 0.5, 1] as const

type Step = 'form' | 'building' | 'signing' | 'success' | 'error'

interface DonateModalProps {
  open: boolean
  onClose: () => void
  walletAddress: string | null
  ergBalanceNano: number
  explorerUrl: string
  onRequestConnect: () => void
  onSuccess?: () => void
}

function parseErgInput(raw: string): bigint | null {
  const t = raw.trim()
  if (!t || !/^\d+(\.\d{0,9})?$/.test(t)) return null
  const [whole, frac = ''] = t.split('.')
  const fracPadded = (frac + '000000000').slice(0, 9)
  try {
    return BigInt(whole) * 1_000_000_000n + BigInt(fracPadded)
  } catch {
    return null
  }
}

export function DonateModal({
  open,
  onClose,
  walletAddress,
  ergBalanceNano,
  explorerUrl,
  onRequestConnect,
  onSuccess,
}: DonateModalProps) {
  const [step, setStep] = useState<Step>('form')
  const [preset, setPreset] = useState<number | 'custom'>(0.1)
  const [customErg, setCustomErg] = useState('')
  const [note, setNote] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [tipNano, setTipNano] = useState(0)

  const flow = useTransactionFlow({
    pollStatus: getTxStatus,
    isOpen: open,
    onSuccess: () => {
      setStep('success')
      onSuccess?.()
    },
    onError: (err) => {
      setError(err)
      setStep('error')
    },
    watchParams: {
      protocol: 'citadel',
      operation: 'donate',
      description: 'Citadel tip',
    },
  })

  const amountNano = useMemo(() => {
    if (preset === 'custom') return parseErgInput(customErg)
    return BigInt(Math.round(preset * 1e9))
  }, [preset, customErg])

  const amountValid =
    amountNano != null &&
    amountNano >= BigInt(MIN_BOX_VALUE_NANO)

  const totalNeeded = amountValid && amountNano != null
    ? amountNano + BigInt(WALLET_TX_FEES_NANO)
    : null

  const insufficient =
    totalNeeded != null && BigInt(ergBalanceNano) < totalNeeded

  const resetForm = () => {
    setStep('form')
    setPreset(0.1)
    setCustomErg('')
    setNote('')
    setError(null)
    setLoading(false)
    setTipNano(0)
    flow.reset()
  }

  const handleClose = () => {
    resetForm()
    onClose()
  }

  const handleConfirm = async () => {
    if (!walletAddress) {
      onRequestConnect()
      return
    }
    if (!amountValid || amountNano == null) {
      setError('Enter a valid amount (min 0.001 ERG)')
      return
    }
    if (insufficient) {
      setError('Insufficient ERG for tip + network fees')
      return
    }

    setLoading(true)
    setError(null)
    setStep('building')
    setTipNano(Number(amountNano))

    try {
      const utxos = await invoke<object[]>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const result = await buildDonationTx({
        changeAddress: walletAddress,
        ergNano: amountNano.toString(),
        userUtxos: utxos,
        currentHeight: nodeStatus.chain_height,
      })

      const tipErg = formatErg(Number(amountNano))
      const noteTrim = note.trim().slice(0, 80)
      const msg = noteTrim
        ? `Citadel tip ${tipErg} ERG — ${noteTrim}`
        : `Citadel tip ${tipErg} ERG`

      const signResult = await startSign(result.unsignedTx, msg)
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  if (!open) return null

  const needsWallet = !walletAddress

  return (
    <Modal
      open={open}
      onClose={handleClose}
      title="Buy me a coffee"
      size="sm"
      footer={
        step === 'form' ? (
          <>
            <Button variant="ghost" onClick={handleClose}>
              Cancel
            </Button>
            {needsWallet ? (
              <Button variant="primary" onClick={onRequestConnect}>
                Connect wallet
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={handleConfirm}
                disabled={!amountValid || insufficient || loading}
                loading={loading}
              >
                Send tip
              </Button>
            )}
          </>
        ) : undefined
      }
    >
      <div className="donate-modal">
        {step === 'form' && (
          <>
            <p className="donate-lead">
              Optional thank-you tip for Citadel development. Separate from the automatic{' '}
              {formatErg(DEV_FEE_NANO)} ERG fee on transactions.
            </p>

            {needsWallet ? (
              <p className="donate-connect-hint">Connect a wallet to send a tip.</p>
            ) : (
              <>
                <div className="donate-presets" role="group" aria-label="Tip amount">
                  {PRESETS.map((p) => (
                    <button
                      key={p}
                      type="button"
                      className={`donate-chip ${preset === p ? 'active' : ''}`}
                      onClick={() => setPreset(p)}
                    >
                      {p} ERG
                    </button>
                  ))}
                  <button
                    type="button"
                    className={`donate-chip ${preset === 'custom' ? 'active' : ''}`}
                    onClick={() => setPreset('custom')}
                  >
                    Custom
                  </button>
                </div>

                {preset === 'custom' && (
                  <FormField label="Amount (ERG)" hint="Minimum 0.001 ERG">
                    <Input
                      type="text"
                      inputMode="decimal"
                      value={customErg}
                      onChange={(e) => setCustomErg(e.target.value)}
                      placeholder="0.25"
                      invalid={customErg.length > 0 && !amountValid}
                    />
                  </FormField>
                )}

                <FormField label="Note (optional)" hint="Shown in wallet signing prompt only">
                  <Input
                    type="text"
                    value={note}
                    onChange={(e) => setNote(e.target.value.slice(0, 80))}
                    placeholder="Thanks for Citadel"
                    maxLength={80}
                  />
                </FormField>

                <div className="donate-meta">
                  <div className="donate-meta-row">
                    <span>Tip</span>
                    <span className="mono">
                      {amountValid && amountNano != null ? `${formatErg(Number(amountNano))} ERG` : '—'}
                    </span>
                  </div>
                  <div className="donate-meta-row muted">
                    <span>Miner + Citadel fee on this tx</span>
                    <span className="mono">{formatErg(WALLET_TX_FEES_NANO)} ERG</span>
                  </div>
                  <div className="donate-meta-row muted">
                    <span>To</span>
                    <span className="mono" title={DEFAULT_DEV_FEE_ADDRESS}>
                      {truncateAddress(DEFAULT_DEV_FEE_ADDRESS, 6)}
                    </span>
                  </div>
                  <div className="donate-meta-row muted">
                    <span>Available</span>
                    <span className="mono">{formatErg(ergBalanceNano)} ERG</span>
                  </div>
                </div>

                {insufficient && (
                  <p className="donate-error">Insufficient ERG for tip + fees</p>
                )}
              </>
            )}

            {error && <p className="donate-error">{error}</p>}
          </>
        )}

        {step === 'building' && (
          <div className="donate-center">
            <Spinner />
            <p>Building tip transaction…</p>
          </div>
        )}

        {step === 'signing' && flow.signMethod === 'choose' && (
          <div className="donate-signing">
            <p className="donate-lead">Choose how to sign your {formatErg(tipNano)} ERG tip</p>
            <div className="donate-wallet-options">
              <button type="button" className="donate-wallet-option" onClick={flow.handleNautilusSign}>
                <span className="donate-wallet-option-name">Nautilus</span>
                <span className="donate-wallet-option-desc">Browser extension</span>
              </button>
              <button type="button" className="donate-wallet-option" onClick={flow.handleMobileSign}>
                <span className="donate-wallet-option-name">Mobile wallet</span>
                <span className="donate-wallet-option-desc">Scan ErgoPay QR</span>
              </button>
            </div>
          </div>
        )}

        {step === 'signing' && flow.signMethod === 'nautilus' && (
          <div className="donate-signing">
            <p>Approve the tip in Nautilus</p>
            <p className="donate-muted">Waiting for approval…</p>
            <div className="donate-actions">
              <Button variant="secondary" onClick={flow.handleBackToChoice}>
                Back
              </Button>
              <Button variant="primary" onClick={flow.handleNautilusSign}>
                Open Nautilus again
              </Button>
            </div>
          </div>
        )}

        {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
          <div className="donate-signing">
            <p>Scan with your Ergo wallet</p>
            <div className="donate-qr">
              <QRCodeSVG value={flow.qrUrl} size={180} />
            </div>
            <p className="donate-muted">Waiting for signature…</p>
            <Button variant="secondary" onClick={flow.handleBackToChoice}>
              Back
            </Button>
          </div>
        )}

        {step === 'success' && (
          <div className="donate-center">
            <p className="donate-thanks">Thank you!</p>
            <p className="donate-muted">Your tip was submitted.</p>
            {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
            <Button variant="primary" onClick={handleClose}>
              Done
            </Button>
          </div>
        )}

        {step === 'error' && (
          <div className="donate-center">
            <p className="donate-error">{error || 'Something went wrong'}</p>
            <div className="donate-actions">
              <Button variant="secondary" onClick={handleClose}>
                Close
              </Button>
              <Button
                variant="primary"
                onClick={() => {
                  setError(null)
                  setStep('form')
                  flow.reset()
                }}
              >
                Try again
              </Button>
            </div>
          </div>
        )}
      </div>
    </Modal>
  )
}
