import { useState, useCallback, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  buildArbChain, startArbLegSign, submitArbChain,
  type CircularArb, type ArbChainBuild, type ArbChainSubmitResponse,
} from '../api/arb'
import { getTxStatus } from '../api/types'
import { formatErg } from '../utils/format'

interface ArbExecuteModalProps {
  isOpen: boolean
  onClose: () => void
  arb: CircularArb
  onDone: () => void
}

type Step = 'building' | 'review' | 'signing' | 'submitting' | 'done' | 'error'

/**
 * Executes a circular arb as N pre-built 0-conf chained direct swaps:
 * build all legs from one pool snapshot -> sign each leg in Nautilus
 * (sign-only, nothing broadcast) -> submit all legs in order.
 */
export function ArbExecuteModal({ isOpen, onClose, arb, onDone }: ArbExecuteModalProps) {
  const [step, setStep] = useState<Step>('building')
  const [error, setError] = useState<string | null>(null)
  const [build, setBuild] = useState<ArbChainBuild | null>(null)
  const [signingLeg, setSigningLeg] = useState(0)
  const [requestIds, setRequestIds] = useState<string[]>([])
  const [submitResult, setSubmitResult] = useState<ArbChainSubmitResponse | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => () => stopPolling(), [stopPolling])

  const doBuild = useCallback(async () => {
    setStep('building')
    setError(null)
    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')
      const result = await buildArbChain(
        arb.pool_ids,
        arb.optimal_input_nano,
        utxos,
        nodeStatus.chain_height,
        0,
      )
      setBuild(result)
      setStep('review')
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [arb])

  useEffect(() => {
    if (isOpen) doBuild()
  }, [isOpen, doBuild])

  const signLeg = useCallback(async (legIndex: number, priorRequestIds: string[]) => {
    if (!build) return
    setSigningLeg(legIndex)
    setStep('signing')
    try {
      const leg = build.legs[legIndex]
      const message = `Arb leg ${legIndex + 1}/${build.legs.length}: ${leg.summary.input_amount} ${leg.summary.input_token} -> ${leg.summary.output_amount} ${leg.summary.output_token} (NOT broadcast until all legs signed)`
      const sign = await startArbLegSign(leg.unsignedTx, message)
      const ids = [...priorRequestIds, sign.requestId]
      setRequestIds(ids)
      await invoke('open_nautilus', { nautilusUrl: sign.nautilusUrl })

      // Poll until this leg is signed, then advance or submit.
      stopPolling()
      pollRef.current = setInterval(async () => {
        try {
          const status = await getTxStatus(sign.requestId)
          if (status.status === 'signed') {
            stopPolling()
            if (legIndex + 1 < build.legs.length) {
              await signLeg(legIndex + 1, ids)
            } else {
              await doSubmit(ids)
            }
          } else if (status.status === 'expired' || status.status === 'failed') {
            stopPolling()
            setError(status.error || 'Signing request failed')
            setStep('error')
          }
        } catch {
          // transient poll error -- keep polling
        }
      }, 1500)
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [build, stopPolling])

  const doSubmit = useCallback(async (ids: string[]) => {
    setStep('submitting')
    try {
      const result = await submitArbChain(ids)
      setSubmitResult(result)
      setStep('done')
      onDone()
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [onDone])

  const handleClose = () => {
    stopPolling()
    onClose()
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={handleClose}>
      <div className="modal arb-execute-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Execute Arb: {arb.path_label}</h3>
          <button className="close-btn" onClick={handleClose}>&times;</button>
        </div>

        <div className="modal-content">
          {step === 'building' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <p>Re-fetching pools and pre-building {arb.pool_ids.length} chained legs...</p>
            </div>
          )}

          {step === 'review' && build && (
            <>
              <div className="preview-section">
                <div className="preview-row highlight">
                  <span>Projected profit (fresh)</span>
                  <span>{formatErg(build.projectedProfitNano)} ERG</span>
                </div>
                <div className="preview-row">
                  <span>Scanned estimate</span>
                  <span>{formatErg(arb.net_profit_nano)} ERG</span>
                </div>
                <div className="preview-row">
                  <span>Input</span>
                  <span>{formatErg(arb.optimal_input_nano)} ERG</span>
                </div>
              </div>

              <div className="arb-exec-legs">
                {build.legs.map((leg, idx) => (
                  <div key={leg.txId} className="arb-exec-leg">
                    <span className="arb-exec-leg-index">Leg {idx + 1}</span>
                    <span className="arb-exec-leg-swap">
                      {leg.summary.input_amount} {leg.summary.input_token} &rarr; {leg.summary.output_amount} {leg.summary.output_token}
                    </span>
                  </div>
                ))}
              </div>

              <div className="warning-box">
                You will sign {build.legs.length} transactions in Nautilus.
                Nothing is broadcast until every leg is signed. Legs execute
                sequentially and are NOT atomic: if a pool moves before
                submission, later legs fail and you keep the intermediate token.
              </div>

              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Cancel</button>
                <button className="btn btn-primary" onClick={() => signLeg(0, [])}>
                  Sign {build.legs.length} legs in Nautilus
                </button>
              </div>
            </>
          )}

          {step === 'signing' && build && (
            <div className="mint-signing-step">
              <div className="arb-exec-stepper">
                {build.legs.map((_, idx) => (
                  <span
                    key={idx}
                    className={`arb-exec-step ${idx < signingLeg ? 'done' : idx === signingLeg ? 'active' : ''}`}
                  >
                    {idx < signingLeg ? '✓' : idx + 1}
                  </span>
                ))}
              </div>
              <div className="spinner-small" />
              <p>Sign leg {signingLeg + 1} of {build.legs.length} in Nautilus...</p>
              <p className="arb-exec-hint">The Nautilus window opened in your browser. Nothing broadcasts yet.</p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Abort (nothing broadcast)</button>
              </div>
            </div>
          )}

          {step === 'submitting' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <p>All legs signed. Broadcasting chain in order...</p>
            </div>
          )}

          {step === 'done' && submitResult && (
            <>
              {submitResult.failedLeg === null ? (
                <div className="preview-section">
                  <div className="preview-row highlight">
                    <span>Arb chain submitted</span>
                    <span>{submitResult.txIds.length} txs</span>
                  </div>
                  {submitResult.txIds.map((txId, idx) => (
                    <div key={txId} className="preview-row">
                      <span>Leg {idx + 1}</span>
                      <span className="arb-exec-txid" title={txId}>{txId.slice(0, 16)}...</span>
                    </div>
                  ))}
                </div>
              ) : (
                <>
                  <div className="message error">
                    {submitResult.error} — {submitResult.txIds.length} of {requestIds.length} legs
                    landed. You may be holding an intermediate token; re-scan or
                    swap it back manually.
                  </div>
                  {submitResult.txIds.map((txId, idx) => (
                    <div key={txId} className="preview-row">
                      <span>Leg {idx + 1} (landed)</span>
                      <span className="arb-exec-txid" title={txId}>{txId.slice(0, 16)}...</span>
                    </div>
                  ))}
                </>
              )}
              <div className="button-group">
                <button className="btn btn-primary" onClick={handleClose}>Close</button>
              </div>
            </>
          )}

          {step === 'error' && (
            <>
              <div className="message error">{error}</div>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Close</button>
                <button className="btn btn-primary" onClick={doBuild}>Rebuild</button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
