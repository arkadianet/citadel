import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildSendTx,
  getRecentTransactions,
  startSign,
  getTxStatus,
  validateErgoAddress,
  type RecentTx,
  type SendBuildResponse,
} from '../api/wallet'
import { formatErg, formatTokenAmount, truncateAddress } from '../utils/format'
import {
  artworkUrlFromRegisters,
  isNftLikeToken,
  resolveMediaUrl,
} from '../utils/eip4'
import { getCachedTokenInfo } from '../api/tokenCache'
import { getBox, type TokenInfo } from '../api/explorer'
import { MIN_BOX_VALUE_NANO, TX_FEE_NANO } from '../constants'
import { Tabs, EmptyState } from './ui'
import { TxSuccess } from './TxSuccess'
import { UtxoManagementTab } from './UtxoManagementTab'
import { BurnTab } from './BurnTab'
import './WalletTab.css'

interface WalletBalance {
  address: string
  addresses?: string[]
  erg_nano: number
  erg_formatted: string
  tokens: Array<{
    token_id: string
    amount: number
    amount_str?: string
    name: string | null
    decimals: number
    pending_amount?: number
  }>
  pending_erg_nano?: number
}

type SubTab = 'overview' | 'nfts' | 'receive' | 'send' | 'activity' | 'utxos' | 'burn'

interface WalletTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletAddressCount: number
  walletBalance: WalletBalance | null
  explorerUrl: string
  /** Optional ERG/USD for nested UTXO fiat display. */
  ergUsdPrice?: number
  onRequestConnect?: () => void
  /** Optional deep-link into a Wallet sub-tab (e.g. from legacy sidebar routes). */
  initialSubTab?: SubTab
  onBalanceRefresh?: () => void
}

type SendStep = 'form' | 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'
type AssetKind = 'erg' | 'token'

interface NftMeta {
  tokenId: string
  name: string
  description: string | null
  boxId: string | null
  imageUrl: string | null
  loading: boolean
  error: string | null
}


function formatTimeAgo(timestampMs: number): string {
  const diff = Date.now() - timestampMs
  const minutes = Math.floor(diff / 60000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(timestampMs).toLocaleDateString()
}

function tokenRawBig(t: { amount: number; amount_str?: string }): bigint {
  if (t.amount_str !== undefined) return BigInt(t.amount_str)
  return BigInt(Math.trunc(t.amount))
}

function parseDisplayToRaw(display: string, decimals: number): bigint {
  const cleaned = display.replace(/,/g, '').trim()
  if (!cleaned) return 0n
  if (decimals === 0) return BigInt(cleaned)
  const [whole, frac = ''] = cleaned.split('.')
  const padded = (frac + '0'.repeat(decimals)).slice(0, decimals)
  return (whole === '' ? 0n : BigInt(whole)) * 10n ** BigInt(decimals) + BigInt(padded || '0')
}

export function WalletTab({
  isConnected,
  walletAddress,
  walletAddressCount,
  walletBalance,
  explorerUrl,
  ergUsdPrice,
  onRequestConnect,
  initialSubTab,
  onBalanceRefresh,
}: WalletTabProps) {
  const [subTab, setSubTab] = useState<SubTab>(initialSubTab ?? 'overview')

  useEffect(() => {
    if (initialSubTab) setSubTab(initialSubTab)
  }, [initialSubTab])
  const [copied, setCopied] = useState(false)
  const [receiveAddress, setReceiveAddress] = useState<string>('')

  // Activity
  const [recentTxs, setRecentTxs] = useState<RecentTx[]>([])
  const [txsLoading, setTxsLoading] = useState(false)
  const [txsError, setTxsError] = useState<string | null>(null)

  // Send form
  const [sendStep, setSendStep] = useState<SendStep>('form')
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [recipient, setRecipient] = useState('')
  const [assetKind, setAssetKind] = useState<AssetKind>('erg')
  const [tokenId, setTokenId] = useState('')
  const [amount, setAmount] = useState('')
  const [sendError, setSendError] = useState<string | null>(null)
  const [sendSummary, setSendSummary] = useState<SendBuildResponse | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  // NFT metadata (EIP-4 / node token + issuance box R9)
  const [nftMeta, setNftMeta] = useState<Record<string, NftMeta>>({})
  const [selectedNftId, setSelectedNftId] = useState<string | null>(null)

  const addresses = useMemo(() => {
    if (walletBalance?.addresses?.length) return walletBalance.addresses
    return walletAddress ? [walletAddress] : []
  }, [walletBalance?.addresses, walletAddress])

  const tokens = walletBalance?.tokens ?? []

  const nftTokens = useMemo(
    () => tokens.filter(t => isNftLikeToken(t)),
    [tokens],
  )
  const fungibleTokens = useMemo(
    () => tokens.filter(t => !isNftLikeToken(t)),
    [tokens],
  )

  useEffect(() => {
    if (walletAddress) setReceiveAddress(walletAddress)
  }, [walletAddress])

  // Resolve NFT names + artwork when the NFTs tab (or overview teaser) needs them
  useEffect(() => {
    let cancelled = false
    const ids = nftTokens.map(t => t.token_id)
    if (ids.length === 0) {
      setNftMeta({})
      setSelectedNftId(null)
      return
    }

    // Seed placeholders so the grid can render immediately
    setNftMeta(prev => {
      const next = { ...prev }
      for (const t of nftTokens) {
        if (!next[t.token_id]) {
          next[t.token_id] = {
            tokenId: t.token_id,
            name: t.name || truncateAddress(t.token_id, 6),
            description: null,
            boxId: null,
            imageUrl: null,
            loading: true,
            error: null,
          }
        }
      }
      // Drop metas for tokens no longer held
      for (const key of Object.keys(next)) {
        if (!ids.includes(key)) delete next[key]
      }
      return next
    })

    ;(async () => {
      for (const t of nftTokens) {
        if (cancelled) return
        try {
          const info: TokenInfo = await getCachedTokenInfo(t.token_id)
          let imageUrl: string | null = null
          let boxId: string | null = info.boxId ?? null
          let description = info.description ?? null
          const name = info.name || t.name || truncateAddress(t.token_id, 6)

          if (boxId) {
            try {
              const box = await getBox(boxId)
              const regs = (box.additionalRegisters ?? {}) as Record<string, string>
              imageUrl = resolveMediaUrl(artworkUrlFromRegisters(regs))
            } catch {
              // Issuance box / R9 may be unavailable on some node tiers
            }
          }

          if (cancelled) return
          setNftMeta(prev => ({
            ...prev,
            [t.token_id]: {
              tokenId: t.token_id,
              name,
              description,
              boxId,
              imageUrl,
              loading: false,
              error: null,
            },
          }))
        } catch (e) {
          if (cancelled) return
          setNftMeta(prev => ({
            ...prev,
            [t.token_id]: {
              tokenId: t.token_id,
              name: t.name || truncateAddress(t.token_id, 6),
              description: null,
              boxId: null,
              imageUrl: null,
              loading: false,
              error: String(e),
            },
          }))
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [nftTokens])

  useEffect(() => {
    setSendStep('form')
    setRecipient('')
    setAmount('')
    setTokenId('')
    setAssetKind('erg')
    setSendError(null)
    setSendSummary(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setRequestId(null)
    setTxId(null)
    setSignMethod('choose')
  }, [walletAddress])

  const fetchActivity = useCallback(async () => {
    if (!walletAddress) {
      setRecentTxs([])
      return
    }
    setTxsLoading(true)
    setTxsError(null)
    try {
      const res = await getRecentTransactions(25)
      setRecentTxs(res.transactions)
    } catch (e) {
      setTxsError(String(e))
      setRecentTxs([])
    } finally {
      setTxsLoading(false)
    }
  }, [walletAddress])

  useEffect(() => {
    if (subTab === 'activity' || subTab === 'overview') {
      fetchActivity()
    }
  }, [subTab, fetchActivity])

  useEffect(() => {
    if (sendStep !== 'signing' || !requestId) return
    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await getTxStatus(requestId)
        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setSendStep('success')
          onBalanceRefresh?.()
        } else if (status.status === 'failed' || status.status === 'expired') {
          setSendError(status.error || 'Transaction failed')
          setSendStep('error')
        }
      } catch (e) {
        console.error('Poll error:', e)
      } finally {
        isPolling = false
      }
    }
    const interval = setInterval(poll, 2000)
    return () => clearInterval(interval)
  }, [sendStep, requestId, onBalanceRefresh])

  const copyAddress = async (addr: string) => {
    try {
      await navigator.clipboard.writeText(addr)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      /* ignore */
    }
  }

  const selectedToken = tokens.find(t => t.token_id === tokenId)

  const parsedSend = useMemo(() => {
    try {
      if (assetKind === 'erg') {
        const raw = parseDisplayToRaw(amount, 9)
        return { ergNano: raw, tokenAmount: null as bigint | null, ok: raw >= BigInt(MIN_BOX_VALUE_NANO) }
      }
      if (!selectedToken) return { ergNano: BigInt(MIN_BOX_VALUE_NANO), tokenAmount: null, ok: false }
      const raw = parseDisplayToRaw(amount, selectedToken.decimals)
      return {
        ergNano: BigInt(MIN_BOX_VALUE_NANO),
        tokenAmount: raw,
        ok: raw > 0n && raw <= tokenRawBig(selectedToken),
      }
    } catch {
      return { ergNano: 0n, tokenAmount: null, ok: false }
    }
  }, [assetKind, amount, selectedToken])

  const canPreviewSend =
    recipient.trim().length > 20 &&
    parsedSend.ok &&
    (assetKind === 'erg' || !!tokenId)

  const resetSend = () => {
    setSendStep('form')
    setSendError(null)
    setSendSummary(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setRequestId(null)
    setTxId(null)
    setSignMethod('choose')
  }

  const handlePreviewSend = async () => {
    setSendError(null)
    setLoading(true)
    try {
      await validateErgoAddress(recipient.trim())
      if (!walletAddress) throw new Error('No wallet connected')
      if (!parsedSend.ok) throw new Error('Invalid amount')

      // Soft balance check
      if (assetKind === 'erg') {
        const need = parsedSend.ergNano + BigInt(TX_FEE_NANO)
        if (BigInt(walletBalance?.erg_nano ?? 0) < need) {
          throw new Error('Insufficient ERG for amount + fee')
        }
      }

      setSendStep('confirm')
    } catch (e) {
      setSendError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const handleSend = async () => {
    if (!walletAddress) return
    setLoading(true)
    setSendError(null)
    setSendStep('building')
    try {
      const utxos = await invoke<object[]>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const result = await buildSendTx({
        recipientAddress: recipient.trim(),
        changeAddress: walletAddress,
        ergNano: parsedSend.ergNano.toString(),
        tokenId: assetKind === 'token' ? tokenId : undefined,
        tokenAmount:
          assetKind === 'token' && parsedSend.tokenAmount != null
            ? parsedSend.tokenAmount.toString()
            : undefined,
        userUtxos: utxos,
        currentHeight: nodeStatus.chain_height,
      })

      setSendSummary(result)

      const msg =
        assetKind === 'token' && selectedToken
          ? `Send ${amount} ${selectedToken.name || 'token'}`
          : `Send ${amount} ERG`
      const signResult = await startSign(result.unsignedTx, msg)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setSendStep('signing')
    } catch (e) {
      setSendError(String(e))
      setSendStep('error')
    } finally {
      setLoading(false)
    }
  }

  const handleNautilusSign = async () => {
    if (!nautilusUrl) return
    setSignMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      setSendError(String(e))
    }
  }

  const header = (
    <header className="wallet-header">
      <div className="wallet-header-left">
        <div className="wallet-icon" aria-hidden>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="18" height="18">
            <path d="M20 7H4a2 2 0 00-2 2v10a2 2 0 002 2h16a2 2 0 002-2V9a2 2 0 00-2-2z" />
            <path d="M16 3v4M8 3v4" />
            <circle cx="16" cy="14" r="1.5" fill="currentColor" stroke="none" />
          </svg>
        </div>
        <div>
          <h1 className="wallet-title">Wallet</h1>
          <p className="wallet-subtitle">Balances, send/receive, UTXOs, and burn via Nautilus / ErgoPay</p>
        </div>
      </div>
    </header>
  )

  if (!isConnected || !walletAddress) {
    return (
      <div className="wallet-tab">
        {header}
        <div className="wallet-empty-wrap">
          <EmptyState
            title="Connect your wallet"
            description="Citadel uses Nautilus or a mobile ErgoPay wallet — no keys are stored here."
            action={
              onRequestConnect ? (
                <button type="button" className="wallet-primary-btn" onClick={onRequestConnect}>
                  Connect wallet
                </button>
              ) : undefined
            }
          />
        </div>
      </div>
    )
  }

  const tabs = (
    <Tabs
      size="compact"
      tabs={[
        { id: 'overview', label: 'Overview' },
        { id: 'nfts', label: nftTokens.length ? `NFTs (${nftTokens.length})` : 'NFTs' },
        { id: 'receive', label: 'Receive' },
        { id: 'send', label: 'Send' },
        { id: 'activity', label: 'Activity' },
        { id: 'utxos', label: 'UTXOs' },
        { id: 'burn', label: 'Burn' },
      ]}
      activeId={subTab}
      onChange={id => setSubTab(id as SubTab)}
    />
  )

  return (
    <div className="wallet-tab">
      {header}
      {tabs}

      {subTab === 'overview' && (
        <div className="wallet-panel">
          <section className="wallet-card wallet-address-card">
            <div className="wallet-card-label">Primary address</div>
            <div className="wallet-address-row">
              <code className="mono wallet-address-text" title={walletAddress}>
                {walletAddress}
              </code>
              <button type="button" className="wallet-secondary-btn" onClick={() => copyAddress(walletAddress)}>
                {copied ? 'Copied' : 'Copy'}
              </button>
            </div>
            {walletAddressCount > 1 && (
              <p className="wallet-muted">
                {walletAddressCount} addresses connected · balances aggregated
              </p>
            )}
          </section>

          <section className="wallet-balances">
            <div className="wallet-balance-hero">
              <span className="wallet-card-label">ERG</span>
              <span className="mono wallet-balance-value">
                {formatErg(walletBalance?.erg_nano ?? 0, 2, 6)}
              </span>
              {(walletBalance?.pending_erg_nano ?? 0) !== 0 && (
                <span className="wallet-pending">
                  pending {(walletBalance!.pending_erg_nano! > 0 ? '+' : '')}
                  {formatErg(walletBalance!.pending_erg_nano!, 2, 4)}
                </span>
              )}
            </div>

            <div className="wallet-token-list">
              {fungibleTokens.length === 0 ? (
                <p className="wallet-muted">No fungible tokens</p>
              ) : (
                fungibleTokens
                  .slice()
                  .sort((a, b) => Number(tokenRawBig(b) - tokenRawBig(a)))
                  .map(t => (
                    <div key={t.token_id} className="wallet-token-row">
                      <div className="wallet-token-meta">
                        <span className="wallet-token-name">
                          {t.name || truncateAddress(t.token_id, 6)}
                        </span>
                        <span className="mono wallet-token-id">{truncateAddress(t.token_id, 6)}</span>
                      </div>
                      <span className="mono wallet-token-amt">
                        {formatTokenAmount(tokenRawBig(t), t.decimals, 0, Math.min(t.decimals, 6))}
                      </span>
                    </div>
                  ))
              )}
            </div>
          </section>

          {nftTokens.length > 0 && (
            <section className="wallet-card">
              <div className="wallet-card-head">
                <span className="wallet-card-label">NFTs</span>
                <button type="button" className="wallet-link-btn" onClick={() => setSubTab('nfts')}>
                  View all ({nftTokens.length})
                </button>
              </div>
              <div className="wallet-nft-strip">
                {nftTokens.slice(0, 6).map(t => {
                  const meta = nftMeta[t.token_id]
                  return (
                    <button
                      key={t.token_id}
                      type="button"
                      className="wallet-nft-chip"
                      onClick={() => {
                        setSelectedNftId(t.token_id)
                        setSubTab('nfts')
                      }}
                      title={meta?.name || t.token_id}
                    >
                      <span className="wallet-nft-chip-art" aria-hidden>
                        {meta?.imageUrl ? (
                          <img src={meta.imageUrl} alt="" loading="lazy" />
                        ) : (
                          <span className="wallet-nft-placeholder">{(meta?.name || '?').slice(0, 1)}</span>
                        )}
                      </span>
                      <span className="wallet-nft-chip-name">
                        {meta?.name || t.name || truncateAddress(t.token_id, 4)}
                      </span>
                    </button>
                  )
                })}
              </div>
            </section>
          )}

          <section className="wallet-card">
            <div className="wallet-card-head">
              <span className="wallet-card-label">Recent activity</span>
              <button type="button" className="wallet-link-btn" onClick={() => setSubTab('activity')}>
                View all
              </button>
            </div>
            {txsLoading && <p className="wallet-muted">Loading…</p>}
            {!txsLoading && recentTxs.length === 0 && (
              <p className="wallet-muted">No recent transactions</p>
            )}
            <div className="wallet-tx-list">
              {recentTxs.slice(0, 5).map(tx => (
                <TxRow key={tx.tx_id} tx={tx} explorerUrl={explorerUrl} />
              ))}
            </div>
          </section>
        </div>
      )}

      {subTab === 'nfts' && (
        <div className="wallet-panel wallet-nfts">
          {nftTokens.length === 0 ? (
            <EmptyState
              title="No NFTs detected"
              description="Tokens with amount 1 and 0 decimals show up here. Protocol singleton NFTs may appear too."
            />
          ) : (
            <div className={`wallet-nft-layout${selectedNftId ? ' has-detail' : ''}`}>
              <div className="wallet-nft-grid" role="list">
                {nftTokens.map(t => {
                  const meta = nftMeta[t.token_id]
                  const selected = selectedNftId === t.token_id
                  return (
                    <button
                      key={t.token_id}
                      type="button"
                      role="listitem"
                      className={`wallet-nft-card${selected ? ' selected' : ''}`}
                      onClick={() => setSelectedNftId(t.token_id)}
                    >
                      <div className="wallet-nft-art">
                        {meta?.loading && <span className="wallet-nft-art-loading">…</span>}
                        {!meta?.loading && meta?.imageUrl && (
                          <img
                            src={meta.imageUrl}
                            alt={meta.name}
                            loading="lazy"
                            onError={e => {
                              ;(e.currentTarget as HTMLImageElement).style.display = 'none'
                            }}
                          />
                        )}
                        {!meta?.loading && !meta?.imageUrl && (
                          <span className="wallet-nft-placeholder lg">
                            {(meta?.name || t.name || '?').slice(0, 2)}
                          </span>
                        )}
                      </div>
                      <div className="wallet-nft-card-meta">
                        <span className="wallet-nft-card-name">
                          {meta?.name || t.name || truncateAddress(t.token_id, 6)}
                        </span>
                        <span className="mono wallet-nft-card-id">{truncateAddress(t.token_id, 6)}</span>
                      </div>
                    </button>
                  )
                })}
              </div>

              {selectedNftId && (() => {
                const t = nftTokens.find(x => x.token_id === selectedNftId)
                const meta = nftMeta[selectedNftId]
                if (!t) return null
                const explorerToken = `${explorerUrl.replace(/\/$/, '')}/en/token/${selectedNftId}`
                const explorerBox = meta?.boxId
                  ? `${explorerUrl.replace(/\/$/, '')}/en/box/${meta.boxId}`
                  : null
                return (
                  <aside className="wallet-nft-detail wallet-card">
                    <div className="wallet-card-head">
                      <span className="wallet-card-label">Details</span>
                      <button type="button" className="wallet-link-btn" onClick={() => setSelectedNftId(null)}>
                        Close
                      </button>
                    </div>
                    <div className="wallet-nft-detail-art">
                      {meta?.imageUrl ? (
                        <img src={meta.imageUrl} alt={meta.name} />
                      ) : (
                        <span className="wallet-nft-placeholder xl">
                          {(meta?.name || '?').slice(0, 2)}
                        </span>
                      )}
                    </div>
                    <h3 className="wallet-nft-detail-title">{meta?.name || t.name || 'Unknown NFT'}</h3>
                    {meta?.description && (
                      <p className="wallet-nft-detail-desc">{meta.description}</p>
                    )}
                    <dl className="wallet-nft-dl">
                      <div>
                        <dt>Token ID</dt>
                        <dd className="mono">{selectedNftId}</dd>
                      </div>
                      {meta?.boxId && (
                        <div>
                          <dt>Issuance box</dt>
                          <dd className="mono">{meta.boxId}</dd>
                        </div>
                      )}
                      <div>
                        <dt>Amount</dt>
                        <dd className="mono">1</dd>
                      </div>
                    </dl>
                    {!meta?.imageUrl && !meta?.loading && (
                      <p className="wallet-muted wallet-nft-gap">
                        Artwork URL not resolved — node token API omits R9; issuance-box registers
                        were missing or not Coll[Byte] UTF-8.
                      </p>
                    )}
                    <div className="wallet-nft-detail-actions">
                      <a className="wallet-secondary-btn" href={explorerToken} target="_blank" rel="noreferrer">
                        Open in explorer
                      </a>
                      {explorerBox && (
                        <a className="wallet-link-btn" href={explorerBox} target="_blank" rel="noreferrer">
                          Issuance box
                        </a>
                      )}
                    </div>
                  </aside>
                )
              })()}
            </div>
          )}
        </div>
      )}

      {subTab === 'receive' && (
        <div className="wallet-panel wallet-receive">
          <section className="wallet-card wallet-receive-card">
            {addresses.length > 1 && (
              <label className="wallet-field">
                <span>Address</span>
                <select
                  value={receiveAddress}
                  onChange={e => setReceiveAddress(e.target.value)}
                >
                  {addresses.map((a, i) => (
                    <option key={a} value={a}>
                      {i === 0 ? `Primary · ${truncateAddress(a, 10)}` : truncateAddress(a, 12)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <div className="wallet-qr-wrap">
              <QRCodeSVG value={receiveAddress || walletAddress} size={180} bgColor="#0b1220" fgColor="#e2e8f0" />
            </div>
            <code className="mono wallet-address-text wallet-receive-addr">
              {receiveAddress || walletAddress}
            </code>
            <button
              type="button"
              className="wallet-primary-btn"
              onClick={() => copyAddress(receiveAddress || walletAddress)}
            >
              {copied ? 'Copied' : 'Copy address'}
            </button>
          </section>
        </div>
      )}

      {subTab === 'send' && (
        <div className="wallet-panel">
          {sendStep === 'form' && (
            <section className="wallet-card wallet-send-form">
              <label className="wallet-field">
                <span>Recipient</span>
                <input
                  type="text"
                  className="mono"
                  placeholder="9..."
                  value={recipient}
                  onChange={e => setRecipient(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>

              <div className="wallet-asset-toggle" role="group" aria-label="Asset">
                <button
                  type="button"
                  className={assetKind === 'erg' ? 'active' : ''}
                  onClick={() => { setAssetKind('erg'); setTokenId(''); setAmount('') }}
                >
                  ERG
                </button>
                <button
                  type="button"
                  className={assetKind === 'token' ? 'active' : ''}
                  onClick={() => setAssetKind('token')}
                >
                  Token
                </button>
              </div>

              {assetKind === 'token' && (
                <label className="wallet-field">
                  <span>Token</span>
                  <select value={tokenId} onChange={e => { setTokenId(e.target.value); setAmount('') }}>
                    <option value="">Select token…</option>
                    {tokens.map(t => (
                      <option key={t.token_id} value={t.token_id}>
                        {t.name || truncateAddress(t.token_id, 6)} ·{' '}
                        {formatTokenAmount(tokenRawBig(t), t.decimals, 0, 4)}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              <label className="wallet-field">
                <span>
                  Amount
                  {assetKind === 'erg' && (
                    <button
                      type="button"
                      className="wallet-max-btn"
                      onClick={() => {
                        const avail = BigInt(walletBalance?.erg_nano ?? 0) - BigInt(TX_FEE_NANO) - BigInt(MIN_BOX_VALUE_NANO)
                        if (avail > 0n) {
                          setAmount(formatTokenAmount(avail, 9, 0, 9).replace(/,/g, ''))
                        }
                      }}
                    >
                      Max
                    </button>
                  )}
                  {assetKind === 'token' && selectedToken && (
                    <button
                      type="button"
                      className="wallet-max-btn"
                      onClick={() => {
                        setAmount(
                          formatTokenAmount(tokenRawBig(selectedToken), selectedToken.decimals, 0, selectedToken.decimals)
                            .replace(/,/g, ''),
                        )
                      }}
                    >
                      Max
                    </button>
                  )}
                </span>
                <input
                  type="text"
                  className="mono"
                  inputMode="decimal"
                  placeholder="0.0"
                  value={amount}
                  onChange={e => setAmount(e.target.value)}
                />
              </label>

              {assetKind === 'token' && (
                <p className="wallet-muted">
                  Recipient receives {formatErg(MIN_BOX_VALUE_NANO)} ERG for the token box · fee{' '}
                  {formatErg(TX_FEE_NANO)} ERG
                </p>
              )}
              {assetKind === 'erg' && (
                <p className="wallet-muted">Network fee ~{formatErg(TX_FEE_NANO)} ERG · change to primary address</p>
              )}

              {sendError && <p className="wallet-error">{sendError}</p>}

              <button
                type="button"
                className="wallet-primary-btn"
                disabled={!canPreviewSend || loading}
                onClick={handlePreviewSend}
              >
                Review send
              </button>
            </section>
          )}

          {sendStep === 'confirm' && (
            <section className="wallet-card wallet-centered-card">
              <h2 className="wallet-section-title">Confirm send</h2>
              <dl className="wallet-confirm-dl">
                <div>
                  <dt>To</dt>
                  <dd className="mono">{truncateAddress(recipient.trim(), 12)}</dd>
                </div>
                <div>
                  <dt>Amount</dt>
                  <dd className="mono">
                    {assetKind === 'erg'
                      ? `${amount} ERG`
                      : `${amount} ${selectedToken?.name || 'token'}`}
                  </dd>
                </div>
                <div>
                  <dt>Fee</dt>
                  <dd className="mono">{formatErg(TX_FEE_NANO)} ERG</dd>
                </div>
              </dl>
              {sendError && <p className="wallet-error">{sendError}</p>}
              <div className="wallet-actions">
                <button type="button" className="wallet-secondary-btn" onClick={resetSend}>
                  Back
                </button>
                <button type="button" className="wallet-primary-btn" disabled={loading} onClick={handleSend}>
                  Sign with wallet
                </button>
              </div>
            </section>
          )}

          {sendStep === 'building' && (
            <section className="wallet-card wallet-centered-card">
              <p className="wallet-muted">Building transaction…</p>
            </section>
          )}

          {sendStep === 'signing' && (
            <section className="wallet-card wallet-centered-card">
              {signMethod === 'choose' && (
                <>
                  <h2 className="wallet-section-title">Sign transaction</h2>
                  {sendSummary && (
                    <p className="wallet-muted">
                      {sendSummary.inputCount} input{sendSummary.inputCount !== 1 ? 's' : ''} · fee{' '}
                      {formatErg(sendSummary.minerFee)} ERG
                      {sendSummary.citadelFeeNano > 0 && (
                        <> · Includes {formatErg(sendSummary.citadelFeeNano)} ERG Citadel fee</>
                      )}
                    </p>
                  )}
                  <div className="wallet-sign-options">
                    <button type="button" className="wallet-sign-option" onClick={handleNautilusSign}>
                      <strong>Nautilus</strong>
                      <span>Browser extension</span>
                    </button>
                    <button type="button" className="wallet-sign-option" onClick={() => setSignMethod('mobile')}>
                      <strong>Mobile</strong>
                      <span>Scan ErgoPay QR</span>
                    </button>
                  </div>
                </>
              )}
              {signMethod === 'mobile' && qrUrl && (
                <>
                  <p className="wallet-muted">Scan with your Ergo wallet</p>
                  <div className="wallet-qr-wrap">
                    <QRCodeSVG value={qrUrl} size={200} bgColor="#0b1220" fgColor="#e2e8f0" />
                  </div>
                  <button type="button" className="wallet-secondary-btn" onClick={() => setSignMethod('choose')}>
                    Back
                  </button>
                </>
              )}
              {signMethod === 'nautilus' && (
                <>
                  <p className="wallet-muted">Waiting for Nautilus…</p>
                  <button type="button" className="wallet-secondary-btn" onClick={() => setSignMethod('choose')}>
                    Choose another method
                  </button>
                </>
              )}
            </section>
          )}

          {sendStep === 'success' && txId && (
            <section className="wallet-card wallet-centered-card">
              <h2 className="wallet-section-title">Sent</h2>
              <TxSuccess txId={txId} explorerUrl={explorerUrl} />
              <button type="button" className="wallet-primary-btn" onClick={resetSend}>
                Send again
              </button>
            </section>
          )}

          {sendStep === 'error' && (
            <section className="wallet-card wallet-centered-card">
              <h2 className="wallet-section-title">Send failed</h2>
              <p className="wallet-error">{sendError}</p>
              <button type="button" className="wallet-primary-btn" onClick={resetSend}>
                Try again
              </button>
            </section>
          )}
        </div>
      )}

      {subTab === 'activity' && (
        <div className="wallet-panel">
          <section className="wallet-card">
            <div className="wallet-card-head">
              <span className="wallet-card-label">Recent transactions</span>
              <button type="button" className="wallet-link-btn" onClick={fetchActivity} disabled={txsLoading}>
                Refresh
              </button>
            </div>
            {txsLoading && <p className="wallet-muted">Loading…</p>}
            {txsError && <p className="wallet-error">{txsError}</p>}
            {!txsLoading && !txsError && recentTxs.length === 0 && (
              <p className="wallet-muted">No transactions found</p>
            )}
            <div className="wallet-tx-list wallet-tx-list--full">
              {recentTxs.map(tx => (
                <TxRow key={tx.tx_id} tx={tx} explorerUrl={explorerUrl} />
              ))}
            </div>
          </section>
        </div>
      )}

      {subTab === 'utxos' && (
        <div className="wallet-nested">
          <UtxoManagementTab
            embedded
            isConnected={isConnected}
            walletAddress={walletAddress}
            walletBalance={walletBalance}
            explorerUrl={explorerUrl}
            ergUsdPrice={ergUsdPrice}
          />
        </div>
      )}

      {subTab === 'burn' && (
        <div className="wallet-nested">
          <BurnTab
            embedded
            isConnected={isConnected}
            walletAddress={walletAddress}
            walletBalance={walletBalance}
            explorerUrl={explorerUrl}
          />
        </div>
      )}
    </div>
  )
}

function TxRow({ tx, explorerUrl }: { tx: RecentTx; explorerUrl: string }) {
  const positive = tx.erg_change_nano > 0
  const negative = tx.erg_change_nano < 0
  return (
    <a
      className="wallet-tx-row"
      href={`${explorerUrl.replace(/\/$/, '')}/en/transactions/${tx.tx_id}`}
      target="_blank"
      rel="noreferrer"
    >
      <div className="wallet-tx-main">
        <span className="mono wallet-tx-id">{truncateAddress(tx.tx_id, 8)}</span>
        <span className="wallet-muted">{formatTimeAgo(tx.timestamp)}</span>
      </div>
      <div className="wallet-tx-deltas">
        {tx.erg_change_nano !== 0 && (
          <span className={`mono ${positive ? 'pos' : ''} ${negative ? 'neg' : ''}`}>
            {positive ? '+' : ''}
            {formatErg(tx.erg_change_nano, 2, 4)} ERG
          </span>
        )}
        {tx.token_changes.slice(0, 2).map(tc => (
          <span key={tc.token_id} className={`mono ${tc.amount > 0 ? 'pos' : 'neg'}`}>
            {tc.amount > 0 ? '+' : ''}
            {formatTokenAmount(tc.amount, tc.decimals, 0, 4)} {tc.name || truncateAddress(tc.token_id, 4)}
          </span>
        ))}
      </div>
    </a>
  )
}
