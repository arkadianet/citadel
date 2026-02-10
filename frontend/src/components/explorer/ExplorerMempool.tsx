import { useState, useEffect, useRef } from 'react'
import { getMempool, formatErg, formatSize, calcFee, type Transaction } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import { TxTypeBadge } from './TxTypeBadge'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  onNavigate: (route: ExplorerRoute) => void
}

type SortKey = 'inputs' | 'outputs' | 'size' | 'fee'

export function ExplorerMempool({ onNavigate }: Props) {
  const [txs, setTxs] = useState<Transaction[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [sortKey, setSortKey] = useState<SortKey>('fee')
  const [sortAsc, setSortAsc] = useState(false)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    const fetch = async () => {
      try {
        const result = await getMempool()
        setTxs(result)
        setError(null)
      } catch (e) {
        setError(String(e))
      } finally {
        setLoading(false)
      }
    }
    fetch()
    intervalRef.current = setInterval(fetch, 5000)
    return () => { if (intervalRef.current) clearInterval(intervalRef.current) }
  }, [])

  const sorted = [...txs].sort((a, b) => {
    let va: number, vb: number
    switch (sortKey) {
      case 'inputs': va = a.inputs.length; vb = b.inputs.length; break
      case 'outputs': va = a.outputs.length; vb = b.outputs.length; break
      case 'size': va = a.size; vb = b.size; break
      case 'fee': va = calcFee(a); vb = calcFee(b); break
    }
    return sortAsc ? va - vb : vb - va
  })

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) setSortAsc(a => !a)
    else { setSortKey(key); setSortAsc(false) }
  }

  const sortIcon = (key: SortKey) =>
    sortKey === key ? (sortAsc ? ' \u25B2' : ' \u25BC') : ''

  if (loading && txs.length === 0) {
    return (
      <div className="explorer-mempool">
        <div className="explorer-section-header">
          <h2 className="explorer-section-title">Mempool</h2>
        </div>
        <ExplorerSkeleton variant="table" rows={5} columns={5} />
      </div>
    )
  }

  return (
    <div className="explorer-mempool">
      <div className="explorer-section-header">
        <h2 className="explorer-section-title">Mempool</h2>
        <span className="explorer-badge">{txs.length} unconfirmed</span>
      </div>

      {error && <div className="explorer-error">{error}</div>}

      <div className="explorer-table-wrap">
        <table className="explorer-table">
          <thead>
            <tr>
              <th>Transaction</th>
              <th style={{ width: '10%' }}>Type</th>
              <th className="sortable text-right" onClick={() => toggleSort('inputs')}>Inputs{sortIcon('inputs')}</th>
              <th className="sortable text-right" onClick={() => toggleSort('outputs')}>Outputs{sortIcon('outputs')}</th>
              <th className="sortable text-right" onClick={() => toggleSort('size')}>Size{sortIcon('size')}</th>
              <th className="sortable text-right" onClick={() => toggleSort('fee')}>Fee{sortIcon('fee')}</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map(tx => (
              <tr key={tx.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'transaction', id: tx.id })}>
                <td className="text-mono text-link">{tx.id.slice(0, 16)}...</td>
                <td><TxTypeBadge tx={tx} /></td>
                <td className="text-right">{tx.inputs.length}</td>
                <td className="text-right">{tx.outputs.length}</td>
                <td className="text-right">{formatSize(tx.size)}</td>
                <td className="text-right">{formatErg(calcFee(tx))} ERG</td>
              </tr>
            ))}
            {txs.length === 0 && !loading && (
              <tr><td colSpan={6} className="text-center text-muted">Mempool is empty</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}
