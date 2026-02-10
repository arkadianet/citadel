import { useState, useEffect, useCallback } from 'react'
import { getAddress, formatErg, calcFee, type AddressInfo } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import { Pagination } from './Pagination'
import { TxTypeBadge } from './TxTypeBadge'
import { TokenPopover } from './TokenPopover'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  address: string
  onNavigate: (route: ExplorerRoute) => void
}

const PAGE_SIZE = 20

export function ExplorerAddress({ address, onNavigate }: Props) {
  const [data, setData] = useState<AddressInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [page, setPage] = useState(0)

  const fetchData = useCallback(async () => {
    setLoading(true)
    try {
      const result = await getAddress(address, page * PAGE_SIZE, PAGE_SIZE)
      setData(result)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [address, page])

  useEffect(() => { fetchData() }, [fetchData])

  if (loading && !data) return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Address</h2>
      <ExplorerSkeleton variant="card" rows={4} />
      <h3 className="explorer-subsection-title">Transactions</h3>
      <ExplorerSkeleton variant="table" rows={6} columns={4} />
    </div>
  )
  if (error && !data) return <div className="explorer-error">{error}</div>
  if (!data) return null

  const totalPages = Math.ceil(data.totalTransactions / PAGE_SIZE)

  return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Address</h2>

      <div className="explorer-info-card">
        <div className="info-row">
          <span className="info-label">Address</span>
          <span className="info-value text-mono text-xs">{data.address}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Balance</span>
          <span className="info-value">
            {formatErg(data.balance.nanoErgs)} ERG
            {data.unconfirmedBalance != null && data.unconfirmedBalance !== 0 && (
              <span className="text-warning ml-2">
                ({data.unconfirmedBalance > 0 ? '+' : ''}{formatErg(data.unconfirmedBalance)} pending)
              </span>
            )}
          </span>
        </div>
        <div className="info-row">
          <span className="info-label">Tokens</span>
          <span className="info-value">{data.balance.tokens.length} token{data.balance.tokens.length !== 1 ? 's' : ''}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Transactions</span>
          <span className="info-value">{data.totalTransactions.toLocaleString()}</span>
        </div>
      </div>

      {/* Token holdings */}
      {data.balance.tokens.length > 0 && (
        <>
          <h3 className="explorer-subsection-title">Token Holdings</h3>
          <div className="explorer-token-grid">
            {data.balance.tokens.map(t => (
              <TokenPopover
                key={t.tokenId}
                tokenId={t.tokenId}
                amount={t.amount}
                onNavigate={onNavigate}
              />
            ))}
          </div>
        </>
      )}

      {/* Unconfirmed transactions */}
      {data.unconfirmedTransactions && data.unconfirmedTransactions.length > 0 && (
        <>
          <h3 className="explorer-subsection-title">
            Unconfirmed
            <span className="explorer-badge">{data.unconfirmedTransactions.length}</span>
          </h3>
          <div className="explorer-table-wrap">
            <table className="explorer-table">
              <thead>
                <tr>
                  <th>Hash</th>
                  <th style={{ width: '10%' }}>Type</th>
                  <th className="text-right">Inputs</th>
                  <th className="text-right">Outputs</th>
                  <th className="text-right">Fee</th>
                </tr>
              </thead>
              <tbody>
                {data.unconfirmedTransactions.map(tx => (
                  <tr key={tx.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'transaction', id: tx.id })}>
                    <td className="text-mono text-link text-warning">{tx.id.slice(0, 16)}...</td>
                    <td><TxTypeBadge tx={tx} /></td>
                    <td className="text-right">{tx.inputs.length}</td>
                    <td className="text-right">{tx.outputs.length}</td>
                    <td className="text-right">{formatErg(calcFee(tx))} ERG</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {/* Transaction history */}
      <h3 className="explorer-subsection-title">
        Transactions
        {loading && <span className="spinner-tiny ml-2" />}
      </h3>
      <div className="explorer-table-wrap">
        <table className="explorer-table">
          <thead>
            <tr>
              <th>Hash</th>
              <th style={{ width: '10%' }}>Type</th>
              <th className="text-right">Inputs</th>
              <th className="text-right">Outputs</th>
              <th className="text-right">Fee</th>
            </tr>
          </thead>
          <tbody>
            {data.transactions.map(tx => (
              <tr key={tx.id} className="explorer-table-row" onClick={() => onNavigate({ page: 'transaction', id: tx.id })}>
                <td className="text-mono text-link">{tx.id.slice(0, 16)}...</td>
                <td><TxTypeBadge tx={tx} /></td>
                <td className="text-right">{tx.inputs.length}</td>
                <td className="text-right">{tx.outputs.length}</td>
                <td className="text-right">{formatErg(calcFee(tx))} ERG</td>
              </tr>
            ))}
            {data.transactions.length === 0 && (
              <tr><td colSpan={5} className="text-center text-muted">No transactions</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Pagination currentPage={page} totalPages={totalPages} onPageChange={setPage} />
    </div>
  )
}
