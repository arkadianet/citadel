import { useState, useEffect, useCallback } from 'react'
import { openExternal } from '../api/external'
import { WalletConnect } from './WalletConnect'
import { MarketCard, type WalletBalance } from './MarketCard'
import { LendModal } from './LendModal'
import { WithdrawModal } from './WithdrawModal'
import { BorrowModal } from './BorrowModal'
import { RepayModal } from './RepayModal'
import { RefundModal } from './RefundModal'
import {
  getLendingMarkets,
  getLendingPositions,
  type PoolInfo,
  type MarketsResponse,
  type PositionsResponse,
  type LendPositionInfo,
  type BorrowPositionInfo,
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
    closeModal()
  }, [fetchMarkets, fetchPositions])

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
        <div className="empty-state">
          <p>Connect to a node first</p>
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="lending-tab">
        <div className="message error">
          Lending requires an indexed node with extraIndex enabled.
        </div>
      </div>
    )
  }

  if (loading && markets.length === 0) {
    return (
      <div className="lending-tab">
        <div className="empty-state">
          <div className="spinner" />
          <p>Loading lending markets...</p>
        </div>
      </div>
    )
  }

  if (error && markets.length === 0) {
    return (
      <div className="lending-tab">
        <div className="message error">{error}</div>
      </div>
    )
  }

  return (
    <div className="lending-tab">
      {/* Wallet Connect Modal */}
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

      {/* Protocol Header */}
      <div className="lending-header">
        <div className="lending-header-row">
          <div className="lending-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </svg>
          </div>
          <div>
            <h2>Lending</h2>
            <p className="lending-description">powered by Duckpools</p>
          </div>
        </div>
      </div>

      {/* Protocol Info Bar */}
      <div className="lending-info-bar">
        <div className="lending-info-item">
          <span className="lending-info-label">Markets:</span>
          <span className="lending-info-value">{markets.length}</span>
        </div>
        <div className="lending-info-divider" />
        <div className="lending-info-item">
          <span className="lending-info-label">Block Height:</span>
          <span className="lending-info-value">{blockHeight.toLocaleString()}</span>
        </div>
        <div className="lending-info-divider" />
        <div className="lending-info-item">
          <span className="lending-info-label">Protocol:</span>
          <span className="lending-info-value">
            <button className="link-button" onClick={() => openExternal('https://duckpools.io')}>
              Duckpools
            </button>
          </span>
        </div>
      </div>

      {/* Mode Toggle */}
      <div className="lending-mode-toggle">
        <button
          className={`mode-btn ${mode === 'supply' ? 'active' : ''}`}
          onClick={() => setMode('supply')}
        >
          Supply
        </button>
        <button
          className={`mode-btn ${mode === 'borrow' ? 'active' : ''}`}
          onClick={() => setMode('borrow')}
        >
          Borrow
        </button>
      </div>

      {/* Markets Grid */}
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

      {/* Wallet Section */}
      {!walletAddress && (
        <div className="lending-wallet-section">
          <div className="wallet-notice">
            <div className="wallet-notice-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="5" width="20" height="14" rx="2" />
                <path d="M2 10h20" />
              </svg>
            </div>
            <h3>Connect Your Wallet</h3>
            <p>Connect your wallet to lend, borrow, and view your positions</p>
            <button className="connect-btn" onClick={() => setShowWalletConnect(true)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="5" width="20" height="14" rx="2" />
                <path d="M2 10h20" />
              </svg>
              Connect Wallet
            </button>
          </div>
        </div>
      )}

      {/* Positions Loading/Error */}
      {walletAddress && positionsLoading && !positions && (
        <div className="lending-positions-loading">
          <div className="spinner-small" />
          <span>Loading your positions...</span>
        </div>
      )}

      {walletAddress && positionsError && (
        <div className="message warning">
          Could not load positions: {positionsError}
        </div>
      )}

      {/* Recover Stuck Transaction Link */}
      <div className="lending-footer">
        <button className="refund-link" onClick={openRefundModal}>
          Recover Stuck Transaction
        </button>
      </div>

      {/* Lend Modal */}
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

      {/* Withdraw Modal */}
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

      {/* Borrow Modal */}
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

      {/* Repay Modal */}
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

      {/* Refund Modal */}
      {activeModal === 'refund' && (
        <RefundModal
          isOpen={true}
          onClose={closeModal}
          userAddress={walletAddress}
          explorerUrl={explorerUrl}
          onSuccess={handleTransactionSuccess}
        />
      )}
    </div>
  )
}

