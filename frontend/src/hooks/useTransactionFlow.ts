import { useState, useEffect, useCallback } from 'react'
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

  // Poll for tx status while signing
  useEffect(() => {
    if (!isSigning || !requestId) return

    let active = true
    const interval = setInterval(async () => {
      try {
        const status = await pollStatus(requestId)
        if (!active) return

        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setIsSigning(false)
          if (watchParams) {
            watchTx(
              status.tx_id,
              watchParams.protocol,
              watchParams.operation,
              watchParams.description,
            ).catch((e) => console.error('Failed to watch tx:', e))
          }
          onSuccess?.(status.tx_id)
        } else if (status.status === 'failed' || status.status === 'expired') {
          setIsSigning(false)
          onError?.(status.error || 'Transaction failed')
        }
      } catch (e) {
        console.error('Poll error:', e)
      }
    }, 2000)

    return () => {
      active = false
      clearInterval(interval)
    }
  }, [isSigning, requestId, pollStatus, onSuccess, onError])

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
