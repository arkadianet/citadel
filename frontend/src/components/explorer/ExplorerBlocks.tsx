import { useState, useEffect, useCallback, useRef } from 'react'
import { getBlockHeaders, formatTimeAgo, formatDifficulty, formatSize, type BlockHeader } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import { Pagination } from './Pagination'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  onNavigate: (route: ExplorerRoute) => void
}

const PAGE_SIZE = 30

export function ExplorerBlocks({ onNavigate }: Props) {
  const [headers, setHeaders] = useState<BlockHeader[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [page, setPage] = useState(0)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const fetchBlocks = useCallback(async () => {
    try {
      const count = PAGE_SIZE * (page + 1) + PAGE_SIZE
      const all = await getBlockHeaders(count)
      // Sort by height descending (newest first)
      all.sort((a, b) => b.height - a.height)
      setHeaders(all)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [page])

  useEffect(() => {
    setLoading(true)
    fetchBlocks()
    intervalRef.current = setInterval(fetchBlocks, 15000)
    return () => { if (intervalRef.current) clearInterval(intervalRef.current) }
  }, [fetchBlocks])

  const start = page * PAGE_SIZE
  const pageHeaders = headers.slice(start, start + PAGE_SIZE)
  // Estimate total pages from highest block height
  const estimatedTotal = headers.length > 0 ? Math.ceil(headers[0].height / PAGE_SIZE) : 1

  if (loading && headers.length === 0) {
    return (
      <div className="explorer-blocks">
        <div className="explorer-section-header">
          <h2 className="explorer-section-title">Recent Blocks</h2>
        </div>
        <ExplorerSkeleton variant="table" rows={10} columns={6} />
      </div>
    )
  }

  if (error && headers.length === 0) {
    return <div className="explorer-error">{error}</div>
  }

  return (
    <div className="explorer-blocks">
      <div className="explorer-section-header">
        <h2 className="explorer-section-title">Recent Blocks</h2>
        {headers.length > 0 && (
          <span className="explorer-badge">Latest: {headers[0].height.toLocaleString()}</span>
        )}
      </div>
      <div className="explorer-table-wrap">
        <table className="explorer-table">
          <thead>
            <tr>
              <th style={{ width: '10%' }}>Height</th>
              <th>Hash</th>
              <th style={{ width: '6%' }} className="text-right">Txns</th>
              <th style={{ width: '10%' }}>Age</th>
              <th style={{ width: '12%' }} className="text-right">Difficulty</th>
              <th style={{ width: '7%' }} className="text-right">Size</th>
            </tr>
          </thead>
          <tbody>
            {pageHeaders.map(h => {
              const nTx = (h as Record<string, unknown>).nTx as number | undefined
              return (
                <tr key={h.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'block', id: h.id })}>
                  <td className="text-mono block-height">{h.height.toLocaleString()}</td>
                  <td className="text-mono text-link text-xs text-truncate">{h.id}</td>
                  <td className="text-right">{nTx ?? '-'}</td>
                  <td className="text-muted">{formatTimeAgo(h.timestamp)}</td>
                  <td className="text-right text-muted">{formatDifficulty(h.difficulty)}</td>
                  <td className="text-right text-muted">{formatSize(h.size)}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>

      <Pagination currentPage={page} totalPages={estimatedTotal} onPageChange={setPage} />
    </div>
  )
}
