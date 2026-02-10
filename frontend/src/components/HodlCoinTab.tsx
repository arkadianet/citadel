import { useState, useEffect, useCallback } from 'react'
import { getHodlCoinBanks, type HodlBankState, formatNanoErg } from '../api/hodlcoin'
import { HodlCoinModal } from './HodlCoinModal'
import './HodlCoinTab.css'

const HODL_ICON_MAP: Record<string, string> = {
  hodlerg3: '/icons/hodlerg3.svg',
  hodlerg: '/icons/hodlerg3.svg',
}

function BankAvatar({ name }: { name: string }) {
  const icon = HODL_ICON_MAP[name.toLowerCase().replace(/\s+/g, '')]
  if (icon) {
    return <img src={icon} alt={name} className="hodl-bank-avatar-img" />
  }
  return <div className="hodl-bank-avatar">H</div>
}

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

interface HodlCoinTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

export function HodlCoinTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  explorerUrl,
}: HodlCoinTabProps) {
  const [banks, setBanks] = useState<HodlBankState[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [selectedBank, setSelectedBank] = useState<HodlBankState | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const fetchBanks = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setLoading(true)
    setError(null)
    try {
      const result = await getHodlCoinBanks()
      setBanks(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier])

  useEffect(() => {
    fetchBanks()
  }, [fetchBanks])

  const openModal = (bank: HodlBankState) => {
    setSelectedBank(bank)
    setModalOpen(true)
  }

  const getUserHodlBalance = (bank: HodlBankState): { raw: number; decimals: number; formatted: string } => {
    if (!walletBalance) return { raw: 0, decimals: 0, formatted: '0' }
    const token = walletBalance.tokens.find(t => t.token_id === bank.hodlTokenId)
    if (!token || token.amount === 0) return { raw: 0, decimals: 0, formatted: '0' }
    const decimals = token.decimals || 0
    const divisor = Math.pow(10, decimals)
    const display = token.amount / divisor
    return {
      raw: token.amount,
      decimals,
      formatted: display.toLocaleString(undefined, {
        minimumFractionDigits: Math.min(2, decimals),
        maximumFractionDigits: Math.max(2, Math.min(decimals, 6)),
      }),
    }
  }

  if (!isConnected || capabilityTier === 'Basic') {
    return (
      <div className="hodl-tab">
        <div className="hodl-header">
          <div className="hodl-header-row">
            <div className="hodl-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="28" height="28">
                <path d="M12 2L2 7l10 5 10-5-10-5z" />
                <path d="M2 17l10 5 10-5" />
                <path d="M2 12l10 5 10-5" />
              </svg>
            </div>
            <div>
              <h2>HodlCoin</h2>
              <p className="hodl-description">Phoenix Hold Coin Protocol</p>
            </div>
          </div>
        </div>
        <div className="message warning">
          Connect to an indexed node to use HodlCoin.
        </div>
      </div>
    )
  }

  return (
    <div className="hodl-tab">
      <div className="hodl-header">
        <div className="hodl-header-row">
          <div className="hodl-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="28" height="28">
              <path d="M12 2L2 7l10 5 10-5-10-5z" />
              <path d="M2 17l10 5 10-5" />
              <path d="M2 12l10 5 10-5" />
            </svg>
          </div>
          <div>
            <h2>HodlCoin</h2>
            <p className="hodl-description">Deposit ERG to mint hodlTokens. The price can only go up over time.</p>
          </div>
        </div>
      </div>

      {loading && banks.length === 0 && (
        <div className="hodl-loading">
          <div className="spinner-small" />
          <span>Discovering banks...</span>
        </div>
      )}

      {error && <div className="message error">{error}</div>}

      {!loading && banks.length === 0 && !error && (
        <div className="message warning">No HodlCoin banks found on the network.</div>
      )}

      <div className="hodl-banks-grid">
        {banks.map(bank => {
          const userBalance = getUserHodlBalance(bank)
          const name = bank.hodlTokenName || `hodl...${bank.hodlTokenId.slice(-6)}`

          return (
            <div key={bank.singletonTokenId} className="hodl-bank-card">
              <div className="hodl-bank-header">
                <div className="hodl-bank-name">
                  <BankAvatar name={name} />
                  <div>
                    <h3>{name}</h3>
                    <span className="hodl-bank-id">{bank.singletonTokenId.slice(0, 12)}...</span>
                  </div>
                </div>
                <span className="hodl-bank-fee">{bank.totalFeePct.toFixed(1)}% fee</span>
              </div>

              <div className="hodl-bank-stats">
                <div className="hodl-stat">
                  <span className="hodl-stat-label">Price</span>
                  <span className="hodl-stat-value">{formatNanoErg(bank.priceNanoPerHodl * 1e9)} ERG</span>
                </div>
                <div className="hodl-stat">
                  <span className="hodl-stat-label">TVL</span>
                  <span className="hodl-stat-value">{formatNanoErg(bank.tvlNanoErg)} ERG</span>
                </div>
                <div className="hodl-stat">
                  <span className="hodl-stat-label">Circulating</span>
                  <span className="hodl-stat-value">{(bank.circulatingSupply / bank.precisionFactor).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })}</span>
                </div>
                <div className="hodl-stat">
                  <span className="hodl-stat-label">Bank Fee</span>
                  <span className="hodl-stat-value">{bank.bankFeePct.toFixed(1)}%</span>
                </div>
                <div className="hodl-stat">
                  <span className="hodl-stat-label">Dev Fee</span>
                  <span className="hodl-stat-value">{bank.devFeePct.toFixed(1)}%</span>
                </div>
              </div>

              {walletAddress && userBalance.raw > 0 && (
                <div className="hodl-balance-box">
                  <span className="hodl-balance-label">Your Balance</span>
                  <span className="hodl-balance-value">{userBalance.formatted} {name}</span>
                </div>
              )}

              <div className="hodl-bank-actions">
                <button
                  className="action-btn primary hodl-primary"
                  disabled={!walletAddress}
                  onClick={() => openModal(bank)}
                  title={!walletAddress ? 'Connect wallet first' : `Mint / Redeem ${name}`}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
                  </svg>
                  Mint / Redeem
                </button>
              </div>
            </div>
          )
        })}
      </div>

      {modalOpen && selectedBank && walletAddress && walletBalance && (
        <HodlCoinModal
          isOpen={modalOpen}
          onClose={() => setModalOpen(false)}
          bank={selectedBank}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          explorerUrl={explorerUrl}
          onSuccess={fetchBanks}
        />
      )}
    </div>
  )
}
