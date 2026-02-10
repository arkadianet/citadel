import { useState, useEffect, useCallback } from 'react'
import {
  fetchBondMarket,
  formatAmount,
  formatPercent,
  blocksToTimeString,
  truncateAddress,
  type BondMarket,
  type OpenOrder,
  type ActiveBond,
} from '../api/sigmafi'
import { SigmaFiConfirmModal, type ConfirmMode } from './SigmaFiConfirmModal'
import { CreateOrderModal } from './CreateOrderModal'
import './SigmaFiTab.css'

interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

interface SigmaFiTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

type SortKey = 'newest' | 'principal' | 'interest' | 'apr' | 'term'
type SubTab = 'orders' | 'bonds'

export function SigmaFiTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  explorerUrl,
}: SigmaFiTabProps) {
  const [market, setMarket] = useState<BondMarket | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [subTab, setSubTab] = useState<SubTab>('orders')
  const [sortKey, setSortKey] = useState<SortKey>('newest')
  const [showOwnOnly, setShowOwnOnly] = useState(false)
  const [hideUndercollateralized, setHideUndercollateralized] = useState(true)

  // Modal state
  const [confirmModalOpen, setConfirmModalOpen] = useState(false)
  const [confirmMode, setConfirmMode] = useState<ConfirmMode>('cancel')
  const [selectedOrder, setSelectedOrder] = useState<OpenOrder | undefined>()
  const [selectedBond, setSelectedBond] = useState<ActiveBond | undefined>()
  const [createModalOpen, setCreateModalOpen] = useState(false)

  const fetchMarket = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setLoading(true)
    setError(null)
    try {
      const result = await fetchBondMarket(walletAddress ?? undefined)
      setMarket(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier, walletAddress])

  useEffect(() => {
    fetchMarket()
  }, [fetchMarket])

  // Action handlers
  const handleCancelOrder = (order: OpenOrder) => {
    setSelectedOrder(order)
    setSelectedBond(undefined)
    setConfirmMode('cancel')
    setConfirmModalOpen(true)
  }

  const handleLendOrder = (order: OpenOrder) => {
    setSelectedOrder(order)
    setSelectedBond(undefined)
    setConfirmMode('lend')
    setConfirmModalOpen(true)
  }

  const handleRepayBond = (bond: ActiveBond) => {
    setSelectedBond(bond)
    setSelectedOrder(undefined)
    setConfirmMode('repay')
    setConfirmModalOpen(true)
  }

  const handleLiquidateBond = (bond: ActiveBond) => {
    setSelectedBond(bond)
    setSelectedOrder(undefined)
    setConfirmMode('liquidate')
    setConfirmModalOpen(true)
  }

  const handleModalSuccess = () => {
    setConfirmModalOpen(false)
    setCreateModalOpen(false)
    fetchMarket()
  }

  const sortedOrders = market ? sortOrders(market.orders, sortKey, showOwnOnly, hideUndercollateralized) : []
  const filteredBonds = market ? filterBonds(market.bonds, showOwnOnly) : []

  if (!isConnected || capabilityTier === 'Basic') {
    return (
      <div className="sigmafi-tab">
        <div className="sigmafi-header">
          <div className="sigmafi-header-row">
            <div className="sigmafi-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M20.42 4.58a5.4 5.4 0 00-7.65 0l-.77.78-.77-.78a5.4 5.4 0 00-7.65 0C1.46 6.7 1.33 10.28 4 13l8 8 8-8c2.67-2.72 2.54-6.3.42-8.42z" />
                <path d="M12 5.36V21" />
              </svg>
            </div>
            <div>
              <h2>SigmaFi Bonds</h2>
              <p className="sigmafi-description">P2P lending with collateralized bonds</p>
            </div>
          </div>
        </div>
        <div className="sigmafi-notice">
          Connect to a full node to use SigmaFi bonds.
        </div>
      </div>
    )
  }

  return (
    <div className="sigmafi-tab">
      {/* Header */}
      <div className="sigmafi-header">
        <div className="sigmafi-header-row">
          <div className="sigmafi-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M20.42 4.58a5.4 5.4 0 00-7.65 0l-.77.78-.77-.78a5.4 5.4 0 00-7.65 0C1.46 6.7 1.33 10.28 4 13l8 8 8-8c2.67-2.72 2.54-6.3.42-8.42z" />
              <path d="M12 5.36V21" />
            </svg>
          </div>
          <div>
            <h2>SigmaFi Bonds</h2>
            <p className="sigmafi-description">P2P lending on Ergo</p>
          </div>
          {walletAddress && (
            <button
              className="sigmafi-create-btn"
              onClick={() => setCreateModalOpen(true)}
            >
              + Create Loan Request
            </button>
          )}
        </div>
      </div>

      {/* Info Bar */}
      {market && (
        <div className="sigmafi-info-bar">
          <div className="sigmafi-info-item">
            <span className="sigmafi-info-label">Open Orders</span>
            <span className="sigmafi-info-value">{market.orders.length}</span>
          </div>
          <div className="sigmafi-info-divider" />
          <div className="sigmafi-info-item">
            <span className="sigmafi-info-label">Active Bonds</span>
            <span className="sigmafi-info-value">{market.bonds.length}</span>
          </div>
          <div className="sigmafi-info-divider" />
          <div className="sigmafi-info-item">
            <span className="sigmafi-info-label">Block Height</span>
            <span className="sigmafi-info-value">{market.blockHeight.toLocaleString()}</span>
          </div>
        </div>
      )}

      {/* Tab Bar + Controls */}
      <div className="sigmafi-tab-bar">
        <button
          className={`sigmafi-tab-btn ${subTab === 'orders' ? 'active' : ''}`}
          onClick={() => setSubTab('orders')}
        >
          Loan Requests ({market?.orders.length ?? 0})
        </button>
        <button
          className={`sigmafi-tab-btn ${subTab === 'bonds' ? 'active' : ''}`}
          onClick={() => setSubTab('bonds')}
        >
          Active Bonds ({market?.bonds.length ?? 0})
        </button>
        <div className="sigmafi-controls">
          <label className="sigmafi-filter-check">
            <input
              type="checkbox"
              checked={showOwnOnly}
              onChange={e => setShowOwnOnly(e.target.checked)}
            />
            My positions
          </label>
          {subTab === 'orders' && (
            <label className="sigmafi-filter-check">
              <input
                type="checkbox"
                checked={hideUndercollateralized}
                onChange={e => setHideUndercollateralized(e.target.checked)}
              />
              Hide undercollateralized
            </label>
          )}
          <button className="sigmafi-refresh-btn" onClick={fetchMarket} disabled={loading}>
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && <div className="sigmafi-error">{error}</div>}

      {/* Orders Sub-tab */}
      {subTab === 'orders' && (
        <>
          <div className="sigmafi-sort-bar">
            <span className="sigmafi-sort-label">Sort:</span>
            {(['newest', 'principal', 'interest', 'apr', 'term'] as SortKey[]).map(key => (
              <button
                key={key}
                className={`sigmafi-sort-btn ${sortKey === key ? 'active' : ''}`}
                onClick={() => setSortKey(key)}
              >
                {key.charAt(0).toUpperCase() + key.slice(1)}
              </button>
            ))}
          </div>
          {loading && !market && (
            <div className="sigmafi-loading">
              <span className="spinner-small" />
              Loading bond market...
            </div>
          )}
          <div className="sigmafi-grid">
            {sortedOrders.length === 0 && !loading && (
              <div className="empty-state">No loan requests found</div>
            )}
            {sortedOrders.map(order => (
              <OrderCard
                key={order.boxId}
                order={order}
                walletAddress={walletAddress}
                onCancel={handleCancelOrder}
                onLend={handleLendOrder}
              />
            ))}
          </div>
        </>
      )}

      {/* Bonds Sub-tab */}
      {subTab === 'bonds' && (
        <>
          {loading && !market && (
            <div className="sigmafi-loading">
              <span className="spinner-small" />
              Loading active bonds...
            </div>
          )}
          <div className="sigmafi-grid">
            {filteredBonds.length === 0 && !loading && (
              <div className="empty-state">No active bonds found</div>
            )}
            {filteredBonds.map(bond => (
              <BondCard
                key={bond.boxId}
                bond={bond}
                onRepay={handleRepayBond}
                onLiquidate={handleLiquidateBond}
              />
            ))}
          </div>
        </>
      )}

      {/* Modals */}
      {walletAddress && (
        <>
          <SigmaFiConfirmModal
            isOpen={confirmModalOpen}
            onClose={() => setConfirmModalOpen(false)}
            onSuccess={handleModalSuccess}
            walletAddress={walletAddress}
            explorerUrl={explorerUrl}
            mode={confirmMode}
            order={selectedOrder}
            bond={selectedBond}
          />
          <CreateOrderModal
            isOpen={createModalOpen}
            onClose={() => setCreateModalOpen(false)}
            onSuccess={handleModalSuccess}
            walletAddress={walletAddress}
            walletBalance={walletBalance}
            explorerUrl={explorerUrl}
          />
        </>
      )}
    </div>
  )
}

// =============================================================================
// Card Components
// =============================================================================

interface OrderCardProps {
  order: OpenOrder
  walletAddress: string | null
  onCancel: (order: OpenOrder) => void
  onLend: (order: OpenOrder) => void
}

function OrderCard({ order, walletAddress, onCancel, onLend }: OrderCardProps) {
  const hasWallet = !!walletAddress

  return (
    <div className={`sigmafi-card ${order.isOwn ? 'own' : ''}`}>
      <div className="sigmafi-card-header">
        <div className="sigmafi-card-header-left">
          <span className="sigmafi-token-badge">{order.loanTokenName}</span>
          {order.isOwn && <span className="sigmafi-own-badge">Your Order</span>}
        </div>
        {order.collateralRatio !== null && (
          <span className={`sigmafi-ratio-badge ${order.collateralRatio >= 150 ? 'good' : order.collateralRatio >= 100 ? 'fair' : 'poor'}`}>
            {order.collateralRatio.toFixed(0)}%
          </span>
        )}
      </div>
      <div className="sigmafi-card-body">
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Principal</span>
          <span className="sigmafi-row-value">{formatAmount(order.principal, order.loanTokenDecimals)} {order.loanTokenName}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Interest</span>
          <span className="sigmafi-row-value highlight">{formatPercent(order.interestPercent)}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">APR</span>
          <span className="sigmafi-row-value">{formatPercent(order.apr)}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Term</span>
          <span className="sigmafi-row-value">{blocksToTimeString(order.maturityBlocks)}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Collateral</span>
          <span className="sigmafi-row-value">{formatAmount(order.collateralErg, 9)} ERG</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Borrower</span>
          <span className="sigmafi-row-value mono">{truncateAddress(order.borrowerAddress)}</span>
        </div>
      </div>
      {hasWallet && (
        <div className="sigmafi-card-actions">
          {order.isOwn ? (
            <button
              className="sigmafi-action-btn danger"
              onClick={() => onCancel(order)}
            >
              Cancel
            </button>
          ) : (
            <button
              className="sigmafi-action-btn primary"
              onClick={() => onLend(order)}
            >
              Lend
            </button>
          )}
        </div>
      )}
    </div>
  )
}

interface BondCardProps {
  bond: ActiveBond
  onRepay: (bond: ActiveBond) => void
  onLiquidate: (bond: ActiveBond) => void
}

function BondCard({ bond, onRepay, onLiquidate }: BondCardProps) {
  const isPastDue = bond.blocksRemaining <= 0
  const role = bond.isOwnLend ? 'Lender' : bond.isOwnBorrow ? 'Borrower' : null

  return (
    <div className={`sigmafi-card ${role ? 'own' : ''} ${isPastDue ? 'past-due' : ''}`}>
      <div className="sigmafi-card-header">
        <div className="sigmafi-card-header-left">
          <span className="sigmafi-token-badge">{bond.loanTokenName}</span>
          {role && <span className="sigmafi-own-badge">{role}</span>}
        </div>
        {isPastDue
          ? <span className="sigmafi-status-badge danger">Past Due</span>
          : <span className="sigmafi-status-badge active">Active</span>
        }
      </div>
      <div className="sigmafi-card-body">
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Repayment</span>
          <span className="sigmafi-row-value">{formatAmount(bond.repayment, bond.loanTokenDecimals)} {bond.loanTokenName}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Time Remaining</span>
          <span className={`sigmafi-row-value ${isPastDue ? 'danger' : ''}`}>
            {isPastDue ? `Overdue ${blocksToTimeString(-bond.blocksRemaining)}` : blocksToTimeString(bond.blocksRemaining)}
          </span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Maturity Height</span>
          <span className="sigmafi-row-value mono">{bond.maturityHeight.toLocaleString()}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Collateral</span>
          <span className="sigmafi-row-value">{formatAmount(bond.collateralErg, 9)} ERG</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Borrower</span>
          <span className="sigmafi-row-value mono">{truncateAddress(bond.borrowerAddress)}</span>
        </div>
        <div className="sigmafi-row">
          <span className="sigmafi-row-label">Lender</span>
          <span className="sigmafi-row-value mono">{truncateAddress(bond.lenderAddress)}</span>
        </div>
      </div>
      {(bond.isRepayable || bond.isLiquidable) && (
        <div className="sigmafi-card-actions">
          {bond.isRepayable && (
            <button
              className="sigmafi-action-btn primary"
              onClick={() => onRepay(bond)}
            >
              Repay
            </button>
          )}
          {bond.isLiquidable && (
            <button
              className="sigmafi-action-btn danger"
              onClick={() => onLiquidate(bond)}
            >
              Liquidate
            </button>
          )}
        </div>
      )}
    </div>
  )
}

// =============================================================================
// Sorting & Filtering
// =============================================================================

function sortOrders(
  orders: OpenOrder[],
  sortKey: SortKey,
  ownOnly: boolean,
  hideUndercollateralized: boolean,
): OpenOrder[] {
  let filtered = ownOnly ? orders.filter(o => o.isOwn) : orders
  if (hideUndercollateralized) {
    filtered = filtered.filter(o =>
      o.isOwn || o.collateralRatio === null || o.collateralRatio >= 100,
    )
  }
  return [...filtered].sort((a, b) => {
    switch (sortKey) {
      case 'newest': return b.creationHeight - a.creationHeight
      case 'principal': return Number(b.principal) - Number(a.principal)
      case 'interest': return b.interestPercent - a.interestPercent
      case 'apr': return b.apr - a.apr
      case 'term': return a.maturityBlocks - b.maturityBlocks
      default: return 0
    }
  })
}

function filterBonds(bonds: ActiveBond[], ownOnly: boolean): ActiveBond[] {
  if (!ownOnly) return bonds
  return bonds.filter(b => b.isOwnLend || b.isOwnBorrow)
}
