import { useState, useEffect, useCallback } from 'react'
import type { TxNotification } from '../api/notifications'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import './Toast.css'

interface ToastStackProps {
  notifications: TxNotification[]
}

interface ToastEntry {
  notif: TxNotification
  visible: boolean
}

const MAX_VISIBLE = 3
const AUTO_DISMISS_MS = 8000

function toastTitle(notif: TxNotification): string {
  switch (notif.kind) {
    case 'confirmed':
      return `${notif.protocol} Confirmed`
    case 'filled':
      return `${notif.protocol} Order Filled`
    case 'dropped':
      return `${notif.protocol} Dropped`
    case 'timeout':
      return `${notif.protocol} Timed Out`
  }
}

function ToastIcon({ kind }: { kind: string }) {
  if (kind === 'confirmed' || kind === 'filled') {
    return (
      <svg className="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
        <polyline points="22 4 12 14.01 9 11.01" />
      </svg>
    )
  }
  if (kind === 'dropped') {
    return (
      <svg className="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <circle cx="12" cy="12" r="10" />
        <line x1="15" y1="9" x2="9" y2="15" />
        <line x1="9" y1="9" x2="15" y2="15" />
      </svg>
    )
  }
  // timeout
  return (
    <svg className="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  )
}

export function ToastStack({ notifications }: ToastStackProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [entries, setEntries] = useState<ToastEntry[]>([])

  // Add new notifications as they arrive
  useEffect(() => {
    if (notifications.length === 0) return
    const latest = notifications[0]
    setEntries((prev) => {
      if (prev.some((e) => e.notif.id === latest.id)) return prev
      return [{ notif: latest, visible: true }, ...prev].slice(0, MAX_VISIBLE)
    })
  }, [notifications])

  // Auto-dismiss
  useEffect(() => {
    if (entries.length === 0) return
    const timer = setTimeout(() => {
      setEntries((prev) => prev.slice(0, -1))
    }, AUTO_DISMISS_MS)
    return () => clearTimeout(timer)
  }, [entries])

  const dismiss = useCallback((id: string) => {
    setEntries((prev) => prev.filter((e) => e.notif.id !== id))
  }, [])

  const handleClick = useCallback(
    (txId: string | null) => {
      if (!txId) return
      navigateToExplorer({ page: 'transaction', id: txId })
    },
    [navigateToExplorer],
  )

  if (entries.length === 0) return null

  return (
    <div className="toast-stack">
      {entries.map((entry) => (
        <div
          key={entry.notif.id}
          className={`toast ${entry.notif.kind}`}
          onClick={() => handleClick(entry.notif.tx_id)}
        >
          <ToastIcon kind={entry.notif.kind} />
          <div className="toast-body">
            <div className="toast-title">{toastTitle(entry.notif)}</div>
            <div className="toast-desc">{entry.notif.description}</div>
          </div>
          <button
            className="toast-close"
            onClick={(e) => {
              e.stopPropagation()
              dismiss(entry.notif.id)
            }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      ))}
    </div>
  )
}
