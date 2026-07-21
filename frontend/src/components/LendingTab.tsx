import { useState, useEffect, useCallback } from 'react'
import { openExternal } from '../api/external'
import { WalletConnect } from './WalletConnect'
import { MarketCard, type WalletBalance } from './MarketCard'
import { LendModal } from './LendModal'
import { WithdrawModal } from './WithdrawModal'
import { BorrowModal } from './BorrowModal'
import { RepayModal } from './RepayModal'
import { RefundModal } from './RefundModal'
import { Tabs, EmptyState } from './ui'
import {
  getLendingMarkets,
  getLendingPositions,
  discoverStuckProxies,
  type PoolInfo,
  type MarketsResponse,
  type PositionsResponse,
  type LendPositionInfo,
  type BorrowPositionInfo,
  type StuckProxyBox,
} from '../api/lending'
import './LendingTab.css'

interface LendingTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  onWalletConnected: (address: string) => void
  explorerUrl: string
}

type ModalType = 'lend' | 'withdraw' | 'borrow' | 'repay' | 'refund' | null
type LendingMode = 'supply' | 'borrow'

export function LendingTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  onWalletConnected,
  explorerUrl,
}: LendingTabProps) {
  // Mode toggle
  const [mode, setMode] = useState<LendingMode>('supply')

  // Markets state
  const [markets, setMarkets] = useState<PoolInfo[]>([])
  const [blockHeight, setBlockHeight] = useState<number>(0)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Positions state (when wallet connected)
  const [positions, setPositions] = useState<PositionsResponse | null>(null)
  const [positionsLoading, setPositionsLoading] = useState(false)
  const [positionsError, setPositionsError] = useState<string | null>(null)

  // Stuck proxy boxes (auto-discovered)
  const [stuckBoxes, setStuckBoxes] = useState<StuckProxyBox[]>([])

  // Modal state
  const [showWalletConnect, setShowWalletConnect] = useState(false)
  const [activeModal, setActiveModal] = useState<ModalType>(null)
  const [selectedPool, setSelectedPool] = useState<PoolInfo | null>(null)
  const [selectedBorrowPosition, setSelectedBorrowPosition] = useState<BorrowPositionInfo | null>(null)

  // Fetch lending markets
  const fetchMarkets = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') {
      return
    }

    setLoading(true)
    setError(null)

    try {
      const response: MarketsResponse = await getLendingMarkets()
      setMarkets(response.pools)
      setBlockHeight(response.block_height)
    } catch (e) {
      console.error('Failed to fetch lending markets:', e)
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier])

  // Fetch user positions
  const fetchPositions = useCallback(async () => {
    if (!walletAddress || !isConnected || capabilityTier === 'Basic') {
      setPositions(null)
      return
    }

    setPositionsLoading(true)
    setPositionsError(null)

    try {
      const response = await getLendingPositions(walletAddress)
      setPositions(response)
    } catch (e) {
      console.error('Failed to fetch lending positions:', e)
      setPositionsError(String(e))
    } finally {
      setPositionsLoading(false)
    }
  }, [walletAddress, isConnected, capabilityTier])

  // Fetch markets on mount and periodically
  useEffect(() => {
    fetchMarkets()
    const interval = setInterval(fetchMarkets, 30000)
    return () => clearInterval(interval)
  }, [fetchMarkets])

  // Fetch positions when wallet is connected
  useEffect(() => {
    if (walletAddress) {
      fetchPositions()
      const interval = setInterval(fetchPositions, 30000)
      return () => clearInterval(interval)
    }
  }, [walletAddress, fetchPositions])

  // Scan for stuck proxy boxes when wallet is connected
  useEffect(() => {
    if (!walletAddress || !isConnected) {
      setStuckBoxes([])
      return
    }
    const scan = async () => {
      try {
        const boxes = await discoverStuckProxies(walletAddress)
        setStuckBoxes(boxes)
      } catch (e) {
        console.error('Failed to scan for stuck proxy boxes:', e)
      }
    }
    scan()
    // Re-scan every 60s (less frequent than positions since this scans ~16 addresses)
    const interval = setInterval(scan, 60000)
    return () => clearInterval(interval)
  }, [walletAddress, isConnected])

  // Modal handlers
  const openLendModal = (pool: PoolInfo) => {
    setSelectedPool(pool)
    setActiveModal('lend')
  }

  const openWithdrawModal = (pool: PoolInfo) => {
    setSelectedPool(pool)
    setActiveModal('withdraw')
  }

  const openBorrowModal = (pool: PoolInfo) => {
    setSelectedPool(pool)
    setActiveModal('borrow')
  }

  const openRepayModal = (pool: PoolInfo, borrowPosition: BorrowPositionInfo) => {
    setSelectedPool(pool)
    setSelectedBorrowPosition(borrowPosition)
    setActiveModal('repay')
  }

  const openRefundModal = () => {
    setActiveModal('refund')
  }

  const closeModal = () => {
    setActiveModal(null)
    setSelectedPool(null)
    setSelectedBorrowPosition(null)
  }

  // Handler for transaction success - will be passed to modal components in Tasks 19-23
  const handleTransactionSuccess = useCallback(() => {
    // Refresh data after successful transaction
    fetchMarkets()
    fetchPositions()
    // Re-scan stuck boxes (a refund may have consumed one)
    if (walletAddress) {
      discoverStuckProxies(walletAddress).then(setStuckBoxes).catch(() => {})
    }
    closeModal()
  }, [fetchMarkets, fetchPositions, walletAddress])

  // Get user's lend position for a pool
  const getLendPosition = (poolId: string): LendPositionInfo | undefined => {
    return positions?.lend_positions.find(p => p.pool_id === poolId)
  }

  // Get user's borrow position for a pool
  const getBorrowPosition = (poolId: string): BorrowPositionInfo | undefined => {
    return positions?.borrow_positions.find(p => p.pool_id === poolId)
  }

  // Render states
  if (!isConnected) {
    return (
      <div className="lending-tab">
        <div className="lending-empty-wrap">
          <EmptyState
            title="Node not connected"
            description="Connect to an Ergo node to access lending markets."
          />
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="lending-tab">
        <div className="lending-empty-wrap">
          <EmptyState
            title="Indexed node required"
            description="Lending needs an indexed node with extraIndex enabled."
          />
        </div>
      </div>
    )
  }

  if (loading && markets.length === 0) {
    return (
      <div className="lending-tab">
        <div className="lending-empty-wrap">
          <div className="lending-positions-loading">
            <div className="spinner-small" />
            <span>Loading lending markets…</span>
          </div>
        </div>
      </div>
    )
  }

  if (error && markets.length === 0) {
    return (
      <div className="lending-tab">
        <div className="lending-empty-wrap">
          <EmptyState title="Error" description={error} />
        </div>
      </div>
    )
  }

  return (
    <div className="lending-tab">
      {showWalletConnect && (
        <div className="modal-overlay" onClick={() => setShowWalletConnect(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Connect Wallet</h2>
              <button className="close-btn" onClick={() => setShowWalletConnect(false)}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12"/>
                </svg>
              </button>
            </div>
            <div className="modal-content">
              <WalletConnect
                onConnected={(address) => {
                  onWalletConnected(address)
                  setShowWalletConnect(false)
                }}
                onCancel={() => setShowWalletConnect(false)}
              />
            </div>
          </div>
        </div>
      )}

      <header className="lending-header">
        <div className="lending-header-left">
          <div className="lending-icon" aria-hidden>
            <img src="/icons/quacks.svg" alt="" />
          </div>
          <div>
            <h1 className="lending-title">Lending</h1>
            <p className="lending-subtitle">
              Duckpools · {markets.length} markets · Block {blockHeight.toLocaleString()}
            </p>
          </div>
        </div>
        <div className="lending-header-actions">
          <Tabs
            tabs={[
              { id: 'supply', label: 'Supply' },
              { id: 'borrow', label: 'Borrow' },
            ]}
            activeId={mode}
            onChange={(id) => setMode(id as 'supply' | 'borrow')}
            size="compact"
          />
          <button className="link-button" onClick={() => openExternal('https://duckpools.io')}>
            Duckpools
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
              <polyline points="15 3 21 3 21 9" />
              <line x1="10" y1="14" x2="21" y2="3" />
            </svg>
          </button>
        </div>
      </header>

      <div className="lending-body">
      <div className="lending-markets-grid">
        {markets.map(pool => (
          <MarketCard
            key={pool.pool_id}
            pool={pool}
            mode={mode}
            lendPosition={getLendPosition(pool.pool_id)}
            borrowPosition={getBorrowPosition(pool.pool_id)}
            walletAddress={walletAddress}
            walletBalance={walletBalance}
            onLend={() => openLendModal(pool)}
            onWithdraw={() => openWithdrawModal(pool)}
            onBorrow={() => openBorrowModal(pool)}
            onRepay={(bp) => openRepayModal(pool, bp)}
          />
        ))}
      </div>

      {!walletAddress && (
        <EmptyState
          title="Wallet not connected"
          description="Connect your wallet to view positions and interact with lending pools."
          action={
            <button className="connect-btn" onClick={() => setShowWalletConnect(true)}>
              Connect Wallet
            </button>
          }
        />
      )}

      {walletAddress && positionsLoading && !positions && (
        <div className="lending-positions-loading">
          <div className="spinner-small" />
          <span>Loading your positions…</span>
        </div>
      )}

      {walletAddress && positionsError && (
        <div className="message warning">
          Could not load positions: {positionsError}
        </div>
      )}

      {stuckBoxes.length > 0 && (
        <div
          className="stuck-boxes-banner"
          onClick={openRefundModal}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openRefundModal() } }}
          role="button"
          tabIndex={0}
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--warning, #f59e0b)" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <path d="M12 8v4" />
            <path d="M12 16h.01" />
          </svg>
          <span>
            You have <strong>{stuckBoxes.length}</strong> stuck proxy box{stuckBoxes.length !== 1 ? 'es' : ''} that can be recovered.
          </span>
          <button className="btn btn-secondary btn-sm">View Details</button>
        </div>
      )}
      </div>

      <div className="lending-footer">
        <button className="refund-link" onClick={openRefundModal}>
          Recover Stuck Transaction
        </button>
      </div>

      {activeModal === 'lend' && selectedPool && walletAddress && (
        <LendModal
          isOpen={true}
          onClose={closeModal}
          pool={selectedPool}
          userAddress={walletAddress}
          walletBalance={walletBalance}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
        />
      )}

      {activeModal === 'withdraw' && selectedPool && walletAddress && (
        <WithdrawModal
          isOpen={true}
          onClose={closeModal}
          pool={selectedPool}
          lendPosition={getLendPosition(selectedPool.pool_id)}
          userAddress={walletAddress}
          walletBalance={walletBalance}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
        />
      )}

      {activeModal === 'borrow' && selectedPool && walletAddress && (
        <BorrowModal
          isOpen={true}
          onClose={closeModal}
          pool={selectedPool}
          userAddress={walletAddress}
          walletBalance={walletBalance}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
        />
      )}

      {activeModal === 'repay' && selectedPool && selectedBorrowPosition && walletAddress && (
        <RepayModal
          isOpen={true}
          onClose={closeModal}
          pool={selectedPool}
          borrowPosition={selectedBorrowPosition}
          userAddress={walletAddress}
          walletBalance={walletBalance}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
        />
      )}

      {activeModal === 'refund' && (
        <RefundModal
          isOpen={true}
          onClose={closeModal}
          userAddress={walletAddress}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
          stuckBoxes={stuckBoxes}
        />
      )}
    </div>
  )
}

