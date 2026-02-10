/**
 * ExplorerDashboard — Node health overview with stats grid and recent blocks.
 *
 * Polls getNodeInfo() every 3 seconds for live data + latency measurement.
 */

import { useState, useEffect, useRef } from 'react'
import {
  getNodeInfo, getBlockHeaders, formatTimeAgo, formatDifficulty,
  type NodeInfo, type BlockHeader,
} from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  onNavigate: (route: ExplorerRoute) => void
}

function getSyncStatus(info: NodeInfo): { label: string; className: string } {
  const diff = info.maxPeerHeight - info.fullHeight
  if (diff <= 1) return { label: 'Synced', className: 'status-badge-green' }
  if (diff <= 10) return { label: 'Syncing', className: 'status-badge-amber' }
  return { label: `Behind (${diff})`, className: 'status-badge-red' }
}

export function ExplorerDashboard({ onNavigate }: Props) {
  const [info, setInfo] = useState<NodeInfo | null>(null)
  const [latency, setLatency] = useState<number | null>(null)
  const [recentBlocks, setRecentBlocks] = useState<BlockHeader[]>([])
  const [loading, setLoading] = useState(true)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    const fetchAll = async () => {
      try {
        const start = performance.now()
        const [nodeInfo, headers] = await Promise.all([
          getNodeInfo(),
          getBlockHeaders(5),
        ])
        setLatency(Math.round(performance.now() - start))
        setInfo(nodeInfo)
        headers.sort((a, b) => b.height - a.height)
        setRecentBlocks(headers.slice(0, 5))
      } catch {
        // Keep previous data on error
      } finally {
        setLoading(false)
      }
    }
    fetchAll()
    intervalRef.current = setInterval(fetchAll, 3000)
    return () => { if (intervalRef.current) clearInterval(intervalRef.current) }
  }, [])

  if (loading && !info) {
    return (
      <div className="explorer-dashboard">
        <h2 className="explorer-section-title">Dashboard</h2>
        <div className="dashboard-stats-grid">
          {Array.from({ length: 8 }, (_, i) => (
            <div key={i} className="dashboard-stat-card">
              <ExplorerSkeleton variant="text" width="60px" />
              <ExplorerSkeleton variant="text" width="100px" />
            </div>
          ))}
        </div>
      </div>
    )
  }

  if (!info) return null

  const sync = getSyncStatus(info)

  return (
    <div className="explorer-dashboard">
      <h2 className="explorer-section-title">Dashboard</h2>

      <div className="dashboard-stats-grid">
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Version</span>
          <span className="dashboard-stat-value">{info.appVersion}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Network</span>
          <span className="dashboard-stat-value">{info.network || 'mainnet'}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Chain Height</span>
          <span className="dashboard-stat-value">{info.fullHeight.toLocaleString()}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Sync Status</span>
          <span className={`dashboard-stat-value ${sync.className}`}>{sync.label}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Latency</span>
          <span className="dashboard-stat-value">{latency != null ? `${latency} ms` : '-'}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Peers</span>
          <span className="dashboard-stat-value">{info.peersCount}</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Mempool</span>
          <span className="dashboard-stat-value">{info.unconfirmedCount} txs</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-label">Mining</span>
          <span className={`dashboard-stat-value ${info.isMining ? 'text-success' : 'text-muted'}`}>
            {info.isMining ? 'Active' : 'Off'}
          </span>
        </div>
      </div>

      {/* Mini recent blocks */}
      <div className="dashboard-recent-header">
        <h3 className="explorer-subsection-title">Recent Blocks</h3>
        <button className="text-link text-xs" onClick={() => onNavigate({ page: 'blocks' })}>
          View all
        </button>
      </div>
      <div className="explorer-table-wrap">
        <table className="explorer-table">
          <thead>
            <tr>
              <th style={{ width: '12%' }}>Height</th>
              <th>Hash</th>
              <th style={{ width: '8%' }} className="text-right">Txns</th>
              <th style={{ width: '12%' }}>Age</th>
              <th style={{ width: '14%' }} className="text-right">Difficulty</th>
            </tr>
          </thead>
          <tbody>
            {recentBlocks.map(h => {
              const nTx = (h as Record<string, unknown>).nTx as number | undefined
              return (
                <tr key={h.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'block', id: h.id })}>
                  <td className="text-mono block-height">{h.height.toLocaleString()}</td>
                  <td className="text-mono text-link text-xs text-truncate">{h.id}</td>
                  <td className="text-right">{nTx ?? '-'}</td>
                  <td className="text-muted">{formatTimeAgo(h.timestamp)}</td>
                  <td className="text-right text-muted">{formatDifficulty(h.difficulty)}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
