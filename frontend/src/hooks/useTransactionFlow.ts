import { useState, useEffect, useCallback, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { TxStatusResponse } from '../api/types'
import { watchTx } from '../api/notifications'

export type SignMethod = 'choose' | 'nautilus' | 'mobile'

export interface WatchParams {
  protocol: string
  operation: string
  description: string
}

interface UseTransactionFlowOptions {
  pollStatus: (requestId: string) => Promise<TxStatusResponse>
  isOpen: boolean
  onSuccess?: (txId: string) => void
  onError?: (error: string) => void
  watchParams?: WatchParams
}

export function useTransactionFlow({ pollStatus, isOpen, onSuccess, onError, watchParams }: UseTransactionFlowOptions) {
  const [requestId, setRequestId] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [txId, setTxId] = useState<string | null>(null)
  const [isSigning, setIsSigning] = useState(false)

  const reset = useCallback(() => {
    setRequestId(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setSignMethod('choose')
    setTxId(null)
    setIsSigning(false)
  }, [])

  // Reset when modal opens
  useEffect(() => {
    if (isOpen) reset()
  }, [isOpen, reset])

  // Refs so the poll effect doesn't reset when parent hands us new inline callbacks
  // on every render. Without this, the 2s interval gets cleared/recreated on each
  // re-render of the parent (e.g. SwapTab's 30s pool refresh + other state churn),
  // and if renders happen faster than 2s the poll never fires.
  const pollStatusRef = useRef(pollStatus)
  const onSuccessRef = useRef(onSuccess)
  const onErrorRef = useRef(onError)
  const watchParamsRef = useRef(watchParams)
  useEffect(() => { pollStatusRef.current = pollStatus }, [pollStatus])
  useEffect(() => { onSuccessRef.current = onSuccess }, [onSuccess])
  useEffect(() => { onErrorRef.current = onError }, [onError])
  useEffect(() => { watchParamsRef.current = watchParams }, [watchParams])

  // Poll for tx status while signing
  useEffect(() => {
    if (!isSigning || !requestId) return

    let active = true
    const interval = setInterval(async () => {
      try {
        const status = await pollStatusRef.current(requestId)
        if (!active) return

        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setIsSigning(false)
          const wp = watchParamsRef.current
          if (wp) {
            watchTx(status.tx_id, wp.protocol, wp.operation, wp.description)
              .catch((e) => console.error('Failed to watch tx:', e))
          }
          onSuccessRef.current?.(status.tx_id)
        } else if (status.status === 'failed' || status.status === 'expired') {
          setIsSigning(false)
          onErrorRef.current?.(status.error || 'Transaction failed')
        }
      } catch (e) {
        console.error('Poll error:', e)
      }
    }, 2000)

    return () => {
      active = false
      clearInterval(interval)
    }
  }, [isSigning, requestId])

  const startSigning = useCallback((rid: string, qr: string, naut: string) => {
    setRequestId(rid)
    setQrUrl(qr)
    setNautilusUrl(naut)
    setIsSigning(true)
    setSignMethod('choose')
  }, [])

  const handleNautilusSign = useCallback(async () => {
    if (!nautilusUrl) return
    setSignMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      console.error('Failed to open Nautilus:', e)
    }
  }, [nautilusUrl])

  const handleMobileSign = useCallback(() => {
    setSignMethod('mobile')
  }, [])

  const handleBackToChoice = useCallback(() => {
    setSignMethod('choose')
  }, [])

  return {
    requestId,
    qrUrl,
    nautilusUrl,
    signMethod,
    txId,
    isSigning,
    startSigning,
    handleNautilusSign,
    handleMobileSign,
    handleBackToChoice,
    reset,
  }
}
