import { useState, useEffect, useCallback, useRef } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import type { TxNotification } from '../api/notifications'
import { getWatchedItems } from '../api/notifications'

export function useNotifications() {
  const [notifications, setNotifications] = useState<TxNotification[]>([])
  const [unreadCount, setUnreadCount] = useState(0)
  const [pendingCount, setPendingCount] = useState(0)
  const permissionChecked = useRef(false)

  // Request OS notification permission once
  useEffect(() => {
    if (permissionChecked.current) return
    permissionChecked.current = true
    ;(async () => {
      if (!(await isPermissionGranted())) {
        await requestPermission()
      }
    })().catch(() => {
      // Non-critical — OS notifications just won't work
    })
  }, [])

  // Listen for tx-notification events from the backend
  useEffect(() => {
    const unlisten = listen<TxNotification>('tx-notification', (event) => {
      const notif = event.payload
      setNotifications((prev) => [notif, ...prev])
      setUnreadCount((c) => c + 1)

      // Send OS notification
      const title =
        notif.kind === 'confirmed'
          ? `${notif.protocol} Confirmed`
          : notif.kind === 'filled'
            ? `${notif.protocol} Order Filled`
            : notif.kind === 'dropped'
              ? `${notif.protocol} Dropped`
              : `${notif.protocol} Timed Out`

      try {
        sendNotification({ title, body: notif.description })
      } catch {
        // Non-critical — OS notifications may not be available
      }
    })

    return () => {
      unlisten.then((fn) => fn())
    }
  }, [])

  // Poll for pending watched items count
  useEffect(() => {
    let active = true
    const poll = async () => {
      try {
        const items = await getWatchedItems()
        if (active) setPendingCount(items.length)
      } catch {
        // Ignore errors
      }
    }

    poll()
    const interval = setInterval(poll, 10_000)
    return () => {
      active = false
      clearInterval(interval)
    }
  }, [])

  const markAllRead = useCallback(() => {
    setUnreadCount(0)
  }, [])

  return { notifications, unreadCount, pendingCount, markAllRead }
}
