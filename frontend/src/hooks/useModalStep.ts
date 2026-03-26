import { useState, useEffect, useCallback, useRef } from 'react'

export type ModalStep = 'preview' | 'building' | 'signing' | 'success' | 'error'

interface UseModalStepOptions<T> {
  isOpen: boolean
  fetchPreview: () => Promise<T>
}

/**
 * Manages the common modal lifecycle: step state, preview fetching,
 * loading/error state, and auto-reset on open.
 *
 * Provides `onTxSuccess` and `onTxError` callbacks suitable for
 * passing directly to `useTransactionFlow`.
 */
export function useModalStep<T>({ isOpen, fetchPreview }: UseModalStepOptions<T>) {
  const [step, setStep] = useState<ModalStep>('preview')
  const [preview, setPreview] = useState<T | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Use a ref so the effect doesn't re-fire when fetchPreview identity changes
  const fetchRef = useRef(fetchPreview)
  fetchRef.current = fetchPreview

  useEffect(() => {
    if (!isOpen) return
    setStep('preview')
    setPreview(null)
    setError(null)
    setLoading(true)
    let cancelled = false
    fetchRef.current()
      .then(result => { if (!cancelled) setPreview(result) })
      .catch(e => { if (!cancelled) setError(String(e)) })
      .finally(() => { if (!cancelled) setLoading(false) })
    return () => { cancelled = true }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen])

  const onTxSuccess = useCallback(() => setStep('success'), [])
  const onTxError = useCallback((err: string) => { setError(err); setStep('error') }, [])

  const reset = useCallback(() => {
    setStep('preview')
    setPreview(null)
    setError(null)
    setLoading(false)
  }, [])

  return { step, setStep, preview, setPreview, loading, setLoading, error, setError, reset, onTxSuccess, onTxError }
}
