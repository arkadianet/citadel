/**
 * ExplorerTab — Built-in blockchain explorer.
 *
 * Sub-routes: dashboard, blocks, block detail, tx detail, address detail,
 * token detail, mempool. Navigation via internal state.
 */

import { useState, useCallback, useEffect, useRef } from 'react'
import { ExplorerDashboard } from './explorer/ExplorerDashboard'
import { ExplorerBlocks } from './explorer/ExplorerBlocks'
import { ExplorerBlock } from './explorer/ExplorerBlock'
import { ExplorerTransaction } from './explorer/ExplorerTransaction'
import { ExplorerAddress } from './explorer/ExplorerAddress'
import { ExplorerToken } from './explorer/ExplorerToken'
import { ExplorerMempool } from './explorer/ExplorerMempool'
import { ExplorerSearch } from './explorer/ExplorerSearch'
import { openExternal } from '../api/external'
import './explorer/Explorer.css'

export type ExplorerRoute =
  | { page: 'dashboard' }
  | { page: 'blocks' }
  | { page: 'mempool' }
  | { page: 'block'; id: string }
  | { page: 'transaction'; id: string }
  | { page: 'address'; id: string }
  | { page: 'token'; id: string }

interface ExplorerTabProps {
  isConnected: boolean
  explorerUrl: string
  pendingRoute?: ExplorerRoute | null
  onPendingRouteConsumed?: () => void
}

export function ExplorerTab({ isConnected, explorerUrl, pendingRoute, onPendingRouteConsumed }: ExplorerTabProps) {
  // Initialize route from pendingRoute on mount so the page renders immediately
  // (no blank flash from a post-render useEffect).
  const [route, setRoute] = useState<ExplorerRoute>(() => pendingRoute ?? { page: 'dashboard' })
  const [history, setHistory] = useState<ExplorerRoute[]>(() =>
    pendingRoute ? [{ page: 'dashboard' }] : []
  )

  const navigate = useCallback((next: ExplorerRoute) => {
    setHistory(h => [...h, route])
    setRoute(next)
  }, [route])

  // Clear the pending route in the parent after mount
  const consumedOnMount = useRef(false)
  useEffect(() => {
    if (!consumedOnMount.current && pendingRoute) {
      consumedOnMount.current = true
      onPendingRouteConsumed?.()
      return
    }
    // Subsequent pending routes (navigated while already on explorer tab)
    if (pendingRoute) {
      navigate(pendingRoute)
      onPendingRouteConsumed?.()
    }
  }, [pendingRoute]) // eslint-disable-line react-hooks/exhaustive-deps

  const goBack = useCallback(() => {
    setHistory(h => {
      const prev = h[h.length - 1]
      if (prev) {
        setRoute(prev)
        return h.slice(0, -1)
      }
      return h
    })
  }, [])

  /** Build the remote explorer URL for the current view */
  const remoteUrl = (() => {
    switch (route.page) {
      case 'dashboard': return `${explorerUrl}`
      case 'blocks': return `${explorerUrl}`
      case 'mempool': return `${explorerUrl}`
      case 'block': return `${explorerUrl}/en/blocks/${route.id}`
      case 'transaction': return `${explorerUrl}/en/transactions/${route.id}`
      case 'address': return `${explorerUrl}/en/addresses/${route.id}`
      case 'token': return `${explorerUrl}/en/token/${route.id}`
    }
  })()

  if (!isConnected) {
    return (
      <div className="explorer-tab">
        <div className="explorer-disconnected">
          <p>Connect to a node to use the explorer.</p>
        </div>
      </div>
    )
  }

  return (
    <div className="explorer-tab">
      {/* Top bar: nav tabs + search + remote link */}
      <div className="explorer-toolbar">
        <div className="explorer-nav">
          {history.length > 0 && (
            <button className="explorer-back-btn" onClick={goBack} title="Go back">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M19 12H5M12 19l-7-7 7-7" />
              </svg>
            </button>
          )}
          <button
            className={`explorer-nav-btn ${route.page === 'dashboard' ? 'active' : ''}`}
            onClick={() => navigate({ page: 'dashboard' })}
          >
            Dashboard
          </button>
          <button
            className={`explorer-nav-btn ${route.page === 'blocks' ? 'active' : ''}`}
            onClick={() => navigate({ page: 'blocks' })}
          >
            Blocks
          </button>
          <button
            className={`explorer-nav-btn ${route.page === 'mempool' ? 'active' : ''}`}
            onClick={() => navigate({ page: 'mempool' })}
          >
            Mempool
          </button>
        </div>

        <ExplorerSearch onNavigate={navigate} />

        <button
          className="explorer-remote-btn"
          onClick={() => openExternal(remoteUrl)}
          title="Open in external explorer"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
            <polyline points="15 3 21 3 21 9" />
            <line x1="10" y1="14" x2="21" y2="3" />
          </svg>
        </button>
      </div>

      {/* Content area */}
      <div className="explorer-content">
        {route.page === 'dashboard' && (
          <ExplorerDashboard onNavigate={navigate} />
        )}
        {route.page === 'blocks' && (
          <ExplorerBlocks onNavigate={navigate} />
        )}
        {route.page === 'mempool' && (
          <ExplorerMempool onNavigate={navigate} />
        )}
        {route.page === 'block' && (
          <ExplorerBlock blockId={route.id} onNavigate={navigate} />
        )}
        {route.page === 'transaction' && (
          <ExplorerTransaction txId={route.id} onNavigate={navigate} explorerUrl={explorerUrl} />
        )}
        {route.page === 'address' && (
          <ExplorerAddress address={route.id} onNavigate={navigate} />
        )}
        {route.page === 'token' && (
          <ExplorerToken tokenId={route.id} onNavigate={navigate} />
        )}
      </div>
    </div>
  )
}
