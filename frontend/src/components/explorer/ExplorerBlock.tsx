import { useState, useEffect } from 'react'
import { getBlock, formatErg, formatSize, formatDifficulty, truncateHash, calcFee, type Block, type Transaction } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import { TxTypeBadge } from './TxTypeBadge'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  blockId: string
  onNavigate: (route: ExplorerRoute) => void
}

export function ExplorerBlock({ blockId, onNavigate }: Props) {
  const [block, setBlock] = useState<Block | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setLoading(true)
    getBlock(blockId)
      .then(b => { setBlock(b); setError(null) })
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false))
  }, [blockId])

  if (loading) return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Block</h2>
      <ExplorerSkeleton variant="card" rows={7} />
      <h3 className="explorer-subsection-title">Transactions</h3>
      <ExplorerSkeleton variant="table" rows={5} columns={5} />
    </div>
  )
  if (error) return <div className="explorer-error">{error}</div>
  if (!block) return null

  const header = block.header
  const txs: Transaction[] = block.blockTransactions?.transactions ?? []
  const ts = new Date(header.timestamp)

  // Miner address from coinbase tx (last tx, last output)
  const coinbaseTx = txs[txs.length - 1]
  const minerOutput = coinbaseTx?.outputs[coinbaseTx.outputs.length - 1]
  const minerAddress = minerOutput?.address

  return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Block</h2>

      <div className="explorer-info-card">
        <div className="info-row">
          <span className="info-label">Block ID</span>
          <span className="info-value text-mono">{header.id}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Height</span>
          <span className="info-value">{header.height.toLocaleString()}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Timestamp</span>
          <span className="info-value">{ts.toLocaleString()}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Transactions</span>
          <span className="info-value">{txs.length}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Size</span>
          <span className="info-value">{formatSize(block.size)}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Difficulty</span>
          <span className="info-value">{formatDifficulty(header.difficulty)}</span>
        </div>
        {header.parentId && (
          <div className="info-row">
            <span className="info-label">Parent</span>
            <span
              className="info-value text-mono text-link"
              onClick={() => onNavigate({ page: 'block', id: header.parentId })}
            >
              {truncateHash(header.parentId)}
            </span>
          </div>
        )}
        {minerAddress && (
          <div className="info-row">
            <span className="info-label">Miner</span>
            <span
              className="info-value text-mono text-link"
              onClick={() => onNavigate({ page: 'address', id: minerAddress })}
            >
              {truncateHash(minerAddress, 10, 8)}
            </span>
          </div>
        )}
      </div>

      <h3 className="explorer-subsection-title">Transactions ({txs.length})</h3>
      <div className="explorer-table-wrap">
        <table className="explorer-table">
          <thead>
            <tr>
              <th>Hash</th>
              <th style={{ width: '10%' }}>Type</th>
              <th className="text-right">Inputs</th>
              <th className="text-right">Outputs</th>
              <th className="text-right">Size</th>
              <th className="text-right">Fee</th>
            </tr>
          </thead>
          <tbody>
            {txs.map(tx => (
              <tr key={tx.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'transaction', id: tx.id })}>
                <td className="text-mono text-link">{tx.id.slice(0, 16)}...</td>
                <td><TxTypeBadge tx={tx} /></td>
                <td className="text-right">{tx.inputs.length}</td>
                <td className="text-right">{tx.outputs.length}</td>
                <td className="text-right">{formatSize(tx.size)}</td>
                <td className="text-right">{formatErg(calcFee(tx))} ERG</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
