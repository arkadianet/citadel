import { useState, useCallback, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  buildSwapChain, startArbLegSign, submitArbChain,
  type SwapChainBuild, type ArbChainSubmitResponse,
} from '../api/arb'
import { getTxStatus } from '../api/types'
import type { RouteQuote } from '../api/router'
import { formatTokenAmount } from '../utils/format'

interface SwapChainModalProps {
  isOpen: boolean
  onClose: () => void
  routeQuote: RouteQuote
  sourceAmount: number // raw units
  onSuccess: () => void
}

type Step = 'building' | 'review' | 'signing' | 'submitting' | 'done' | 'error'

/**
 * Executes a multi-hop smart swap as pre-built 0-conf chained direct swaps
 * (same machinery as arb execution): build all legs from one pool snapshot ->
 * sign each leg in Nautilus (sign-only) -> submit all legs in order.
 */
export function SwapChainModal({ isOpen, onClose, routeQuote, sourceAmount, onSuccess }: SwapChainModalProps) {
  const [step, setStep] = useState<Step>('building')
  const [error, setError] = useState<string | null>(null)
  const [build, setBuild] = useState<SwapChainBuild | null>(null)
  const [signingLeg, setSigningLeg] = useState(0)
  const [requestIds, setRequestIds] = useState<string[]>([])
  const [submitResult, setSubmitResult] = useState<ArbChainSubmitResponse | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const hops = routeQuote.route.hops
  const firstHop = hops[0]
  const lastHop = hops[hops.length - 1]
  const sourceLabel = firstHop.token_in_name || (firstHop.token_in === 'ERG' ? 'ERG' : firstHop.token_in.slice(0, 8))
  const targetLabel = lastHop.token_out_name || (lastHop.token_out === 'ERG' ? 'ERG' : lastHop.token_out.slice(0, 8))

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
      const result = await buildSwapChain(
        hops.map(h => h.pool_id),
        firstHop.token_in === 'ERG' ? null : firstHop.token_in,
        sourceAmount,
        utxos,
        nodeStatus.chain_height,
      )
      setBuild(result)
      setStep('review')
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [routeQuote, sourceAmount])

  useEffect(() => {
    if (isOpen) doBuild()
  }, [isOpen, doBuild])

  const doSubmit = useCallback(async (ids: string[]) => {
    setStep('submitting')
    try {
      const result = await submitArbChain(ids)
      setSubmitResult(result)
      setStep('done')
      if (result.failedLeg === null) onSuccess()
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [onSuccess])

  const signLeg = useCallback(async (legIndex: number, priorRequestIds: string[]) => {
    if (!build) return
    setSigningLeg(legIndex)
    setStep('signing')
    try {
      const leg = build.legs[legIndex]
      const message = `Swap leg ${legIndex + 1}/${build.legs.length}: ${leg.summary.input_amount} ${leg.summary.input_token} -> ${leg.summary.output_amount} ${leg.summary.output_token} (NOT broadcast until all legs signed)`
      const sign = await startArbLegSign(leg.unsignedTx, message)
      const ids = [...priorRequestIds, sign.requestId]
      setRequestIds(ids)
      await invoke('open_nautilus', { nautilusUrl: sign.nautilusUrl })

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
  }, [build, stopPolling, doSubmit])

  const handleClose = () => {
    stopPolling()
    onClose()
  }

  if (!isOpen) return null

  const targetDecimals = lastHop.token_out_decimals ?? 0

  return (
    <div className="modal-overlay" onClick={handleClose}>
      <div className="modal arb-execute-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Multi-hop Swap: {sourceLabel} &rarr; {targetLabel}</h3>
          <button className="close-btn" onClick={handleClose}>&times;</button>
        </div>

        <div className="modal-content">
          {step === 'building' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <p>Pre-building {hops.length} chained legs from fresh pool state...</p>
            </div>
          )}

          {step === 'review' && build && (
            <>
              <div className="preview-section">
                <div className="preview-row highlight">
                  <span>You receive (exact)</span>
                  <span>{formatTokenAmount(build.finalOutput, targetDecimals)} {targetLabel}</span>
                </div>
                <div className="preview-row">
                  <span>Quoted estimate</span>
                  <span>{formatTokenAmount(routeQuote.route.total_output, targetDecimals)} {targetLabel}</span>
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
                Nothing broadcasts until every leg is signed. The amounts above
                are exact: each leg spends a specific pool box, so the chain
                either executes as shown or a leg fails wholesale if someone
                moves that pool first (you keep the intermediate token).
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
                    <span>Swap chain submitted</span>
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
                    landed. You may be holding an intermediate token; re-quote and
                    swap it onward or back.
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
