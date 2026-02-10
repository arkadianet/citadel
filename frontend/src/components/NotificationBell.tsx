import { useState, useRef, useEffect, useCallback } from 'react'
import type { TxNotification } from '../api/notifications'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import './NotificationBell.css'

interface NotificationBellProps {
  notifications: TxNotification[]
  unreadCount: number
  pendingCount: number
  onMarkAllRead: () => void
}

function relativeTime(timestamp: number): string {
  const diff = Math.floor(Date.now() / 1000 - timestamp)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function kindLabel(notif: TxNotification): string {
  switch (notif.kind) {
    case 'confirmed':
      return `${notif.protocol} confirmed`
    case 'filled':
      return `${notif.protocol} order filled`
    case 'dropped':
      return `${notif.protocol} dropped`
    case 'timeout':
      return `${notif.protocol} timed out`
  }
}

export function NotificationBell({
  notifications,
  unreadCount,
  pendingCount,
  onMarkAllRead,
}: NotificationBellProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  // Close on outside click
  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  const handleOpen = useCallback(() => {
    setOpen((o) => !o)
  }, [])

  const handleItemClick = useCallback(
    (txId: string | null) => {
      if (!txId) return
      navigateToExplorer({ page: 'transaction', id: txId })
      setOpen(false)
    },
    [navigateToExplorer],
  )

  const badgeCount = unreadCount || pendingCount

  return (
    <div className="notification-bell" ref={ref}>
      <button className="bell-btn" onClick={handleOpen} title="Notifications">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
          <path d="M13.73 21a2 2 0 01-3.46 0" />
        </svg>
      </button>

      {badgeCount > 0 && <span className="bell-badge">{badgeCount}</span>}

      {open && (
        <div className="bell-dropdown">
          <div className="bell-dropdown-header">
            <h4>Notifications</h4>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              {pendingCount > 0 && (
                <span className="bell-pending">{pendingCount} pending</span>
              )}
              {unreadCount > 0 && (
                <button className="bell-mark-read" onClick={onMarkAllRead}>
                  Mark read
                </button>
              )}
            </div>
          </div>

          <div className="bell-list">
            {notifications.length === 0 ? (
              <div className="bell-empty">
                {pendingCount > 0
                  ? 'Waiting for confirmations...'
                  : 'No notifications yet'}
              </div>
            ) : (
              notifications.slice(0, 20).map((notif) => (
                <div
                  key={notif.id}
                  className="bell-item"
                  onClick={() => handleItemClick(notif.tx_id)}
                >
                  <div className={`bell-item-dot ${notif.kind}`} />
                  <div className="bell-item-content">
                    <div className="bell-item-title">{kindLabel(notif)}</div>
                    <div className="bell-item-desc">{notif.description}</div>
                  </div>
                  <div className="bell-item-time">{relativeTime(notif.timestamp)}</div>
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  )
}
