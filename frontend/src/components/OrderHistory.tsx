import { useState, useEffect, useCallback } from 'react'
import {
  getPendingOrders,
  getMempoolSwaps,
  formatOrderInput,
  type PendingOrder,
  type MempoolSwap,
} from '../api/orders'
import { formatErg } from '../api/amm'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import { SwapRefundModal } from './SwapRefundModal'

interface OrderHistoryProps {
  walletAddress: string | null
  explorerUrl: string
}

type OrderRow =
  | { kind: 'proxy'; order: PendingOrder }
  | { kind: 'direct'; swap: MempoolSwap }

export function OrderHistory({ walletAddress, explorerUrl }: OrderHistoryProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [rows, setRows] = useState<OrderRow[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [refundOrder, setRefundOrder] = useState<PendingOrder | null>(null)

  const fetchOrders = useCallback(async () => {
    if (!walletAddress) return
    setLoading(true)
    try {
      const [proxyOrders, mempoolSwaps] = await Promise.all([
        getPendingOrders(),
        getMempoolSwaps(),
      ])
      const merged: OrderRow[] = [
        ...mempoolSwaps.map((swap): OrderRow => ({ kind: 'direct', swap })),
        ...proxyOrders.map((order): OrderRow => ({ kind: 'proxy', order })),
      ]
      setRows(merged)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [walletAddress])

  useEffect(() => {
    fetchOrders()
    const interval = setInterval(fetchOrders, 10_000)
    return () => clearInterval(interval)
  }, [fetchOrders])

  const handleRefundSuccess = () => {
    setRefundOrder(null)
    fetchOrders()
  }

  if (!walletAddress) return null

  if (loading && rows.length === 0) {
    return (
      <div className="order-history">
        <div className="order-history-loading">
          <div className="spinner-small" />
          <span>Scanning for orders...</span>
        </div>
      </div>
    )
  }

  return (
    <div className="order-history">
      <div className="order-history-header">
        <h3>Pending Orders</h3>
      </div>

      {error && <div className="message error">{error}</div>}

      {rows.length === 0 ? (
        <div className="order-history-empty">
          <p>No pending orders</p>
          <p className="order-history-hint">
            Submitted swap orders and confirming transactions will appear here.
          </p>
        </div>
      ) : (
        <div className="order-table-container">
          <table className="order-table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Details</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(row =>
                row.kind === 'proxy' ? (
                  <ProxyRow
                    key={row.order.boxId}
                    order={row.order}
                    onExplorer={() =>
                      navigateToExplorer({ page: 'transaction', id: row.order.txId })
                    }
                    onRefund={() => setRefundOrder(row.order)}
                  />
                ) : (
                  <DirectRow
                    key={row.swap.txId}
                    swap={row.swap}
                    onExplorer={() =>
                      navigateToExplorer({ page: 'transaction', id: row.swap.txId })
                    }
                  />
                ),
              )}
            </tbody>
          </table>
        </div>
      )}

      {refundOrder && walletAddress && (
        <SwapRefundModal
          isOpen={true}
          onClose={() => setRefundOrder(null)}
          order={refundOrder}
          walletAddress={walletAddress}
          explorerUrl={explorerUrl}
          onSuccess={handleRefundSuccess}
        />
      )}
    </div>
  )
}

function ProxyRow({
  order,
  onExplorer,
  onRefund,
}: {
  order: PendingOrder
  onExplorer: () => void
  onRefund: () => void
}) {
  return (
    <tr>
      <td>
        <span className="order-badge order-badge-proxy">Proxy</span>
      </td>
      <td>
        {formatOrderInput(order.input, order.inputDecimals)} &rarr; min {order.outputDecimals > 0
          ? (order.minOutput / Math.pow(10, order.outputDecimals)).toLocaleString(undefined, { maximumFractionDigits: order.outputDecimals })
          : order.minOutput.toLocaleString()}
      </td>
      <td>
        <span className="order-status-pending">Pending</span>
      </td>
      <td className="order-actions">
        <ExplorerButton onClick={onExplorer} />
        <button className="btn btn-danger btn-sm" onClick={onRefund}>
          Refund
        </button>
      </td>
    </tr>
  )
}

function DirectRow({
  swap,
  onExplorer,
}: {
  swap: MempoolSwap
  onExplorer: () => void
}) {
  const details = formatReceiving(swap)
  return (
    <tr>
      <td>
        <span className="order-badge order-badge-direct">Direct</span>
      </td>
      <td>{details}</td>
      <td>
        <span className="order-status-confirming">Confirming</span>
      </td>
      <td className="order-actions">
        <ExplorerButton onClick={onExplorer} />
      </td>
    </tr>
  )
}

function ExplorerButton({ onClick }: { onClick: () => void }) {
  return (
    <button className="order-link" title="View in explorer" onClick={onClick}>
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      >
        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
        <polyline points="15 3 21 3 21 9" />
        <line x1="10" y1="14" x2="21" y2="3" />
      </svg>
    </button>
  )
}

function formatReceiving(swap: MempoolSwap): string {
  const parts: string[] = []
  if (swap.receivingErg > 0) {
    parts.push(`${formatErg(swap.receivingErg)} ERG`)
  }
  for (const [tokenId, amount, decimals] of swap.receivingTokens) {
    const display = decimals > 0
      ? (amount / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
      : amount.toLocaleString()
    parts.push(`${display} ${tokenId.slice(0, 8)}...`)
  }
  return parts.length > 0 ? `Receiving ${parts.join(' + ')}` : 'Swap confirming'
}
