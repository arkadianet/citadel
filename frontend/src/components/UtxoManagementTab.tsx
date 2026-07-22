import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildConsolidateTx,
  buildSplitTx,
  buildRestructureTx,
  startUtxoMgmtSign,
  getUtxoMgmtTxStatus,
} from '../api/utxoManagement'
import type {
  ConsolidateBuildResponse,
  SplitBuildResponse,
  RestructureBuildResponse,
} from '../api/utxoManagement'
import { getCachedTokenInfo } from '../api/tokenCache'
import {
  TX_FEE_NANO,
  MIN_BOX_VALUE_NANO,
  DEV_FEE_NANO,
  WALLET_TX_FEES_NANO,
  MAX_RESTRUCTURE_OUTPUTS,
} from '../constants'
import { formatErg, formatTokenAmount, truncateAddress } from '../utils/format'
import { isNftLikeToken } from '../utils/eip4'
import { TxSuccess } from './TxSuccess'
import { Tabs, EmptyState } from './ui'
import './UtxoManagementTab.css'

interface UtxoManagementTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
  /** Optional ERG/USD for fiat display on cards/stats. */
  ergUsdPrice?: number
  /** When nested under Wallet, hide the page header and tighten padding. */
  embedded?: boolean
}

type SubTab = 'consolidate' | 'split' | 'restructure'
type Step = 'select' | 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'
type SplitType = 'erg' | 'token'
type BoardFilter = 'all' | 'dust' | 'erg' | 'tokens' | 'nfts' | 'large'
type SortKey = 'value-desc' | 'value-asc' | 'height-desc' | 'height-asc'
type PillKind = 'dust' | 'large' | 'token' | 'nft'

interface RestructureTokenAssign {
  tokenId: string
  /** Display units (respects decimals) */
  amount: string
}

interface RestructureSlot {
  id: string
  /** ERG as decimal string */
  erg: string
  tokens: RestructureTokenAssign[]
}

let restructureSlotSeq = 0
function newRestructureSlot(erg = ''): RestructureSlot {
  restructureSlotSeq += 1
  return { id: `out-${restructureSlotSeq}`, erg, tokens: [] }
}

function parseErgToNano(erg: string): number | null {
  const trimmed = erg.trim().replace(/,/g, '')
  if (trimmed === '' || trimmed === '.') return null
  const parsed = parseFloat(trimmed)
  if (isNaN(parsed) || parsed < 0) return null
  return Math.floor(parsed * 1e9)
}

/** Compact ERG string for inputs (trim trailing zeros). */
function nanoToErgInput(nano: number): string {
  if (nano <= 0) return '0'
  const s = (nano / 1e9).toFixed(9)
  return s.replace(/\.?0+$/, '') || '0'
}

function rawToDisplayAmount(raw: number, decimals: number): string {
  if (decimals <= 0) return String(raw)
  const div = Math.pow(10, decimals)
  const s = (raw / div).toFixed(decimals)
  return s.replace(/\.?0+$/, '') || '0'
}

/** Pin slot `pinnedId`, put leftover spendable ERG on the last other slot. */
function redistributeErg(slots: RestructureSlot[], pinnedId: string, spendableNano: number): RestructureSlot[] {
  if (slots.length <= 1) return slots
  const pinnedIdx = slots.findIndex(s => s.id === pinnedId)
  if (pinnedIdx < 0) return slots

  let absorbIdx = slots.length - 1
  if (absorbIdx === pinnedIdx) absorbIdx = slots.length - 2
  if (absorbIdx < 0) return slots

  const pinnedNano = parseErgToNano(slots[pinnedIdx].erg) ?? 0
  let othersSum = 0
  for (let i = 0; i < slots.length; i++) {
    if (i === pinnedIdx || i === absorbIdx) continue
    othersSum += parseErgToNano(slots[i].erg) ?? 0
  }
  const remainder = spendableNano - pinnedNano - othersSum
  return slots.map((s, i) =>
    i === absorbIdx ? { ...s, erg: nanoToErgInput(Math.max(0, remainder)) } : s,
  )
}

interface UtxoBox {
  boxId: string
  value: string
  ergoTree: string
  assets: Array<{ tokenId: string; amount: string }>
  creationHeight: number
  transactionId: string
  index: number
  additionalRegisters: Record<string, string>
  extension: Record<string, string>
}

/** Mockup: Dust < 1 ERG */
const DUST_NANO = 1_000_000_000
/** Mockup: Large > 10 ERG */
const LARGE_NANO = 10_000_000_000

function ergNano(box: UtxoBox): number {
  return parseInt(box.value || '0', 10) || 0
}

function truncBoxId(id: string): string {
  if (id.length <= 12) return id
  return `${id.slice(0, 4)}…${id.slice(-4)}`
}

function formatFiat(nano: number, ergUsd: number | undefined): string | null {
  if (!ergUsd || ergUsd <= 0) return null
  const usd = (nano / 1e9) * ergUsd
  return `$${usd.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

function boxHasNft(
  box: UtxoBox,
  tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>,
): boolean {
  return box.assets.some(a => {
    const amt = parseInt(a.amount, 10) || 0
    const wt = tokens.find(t => t.token_id === a.tokenId)
    if (wt) {
      return isNftLikeToken({ amount: amt, decimals: wt.decimals })
    }
    return amt === 1
  })
}

function boxHasFungible(
  box: UtxoBox,
  tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>,
): boolean {
  return box.assets.some(a => {
    const amt = parseInt(a.amount, 10) || 0
    const wt = tokens.find(t => t.token_id === a.tokenId)
    if (wt) return !isNftLikeToken({ amount: amt, decimals: wt.decimals })
    return amt !== 1
  })
}

function boxPills(
  box: UtxoBox,
  tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>,
): PillKind[] {
  const pills: PillKind[] = []
  const nano = ergNano(box)
  if (boxHasNft(box, tokens)) pills.push('nft')
  else if (box.assets.length > 0) pills.push('token')
  if (nano < DUST_NANO) pills.push('dust')
  if (nano > LARGE_NANO) pills.push('large')
  return pills
}

function matchesFilter(
  box: UtxoBox,
  filter: BoardFilter,
  tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>,
): boolean {
  const nano = ergNano(box)
  switch (filter) {
    case 'all':
      return true
    case 'dust':
      return nano < DUST_NANO
    case 'erg':
      return box.assets.length === 0
    case 'tokens':
      return boxHasFungible(box, tokens)
    case 'nfts':
      return boxHasNft(box, tokens)
    case 'large':
      return nano > LARGE_NANO
    default:
      return true
  }
}

export function UtxoManagementTab({
  isConnected,
  walletAddress,
  walletBalance,
  explorerUrl,
  ergUsdPrice,
  embedded = false,
}: UtxoManagementTabProps) {
  const [subTab, setSubTab] = useState<SubTab>('consolidate')
  const [step, setStep] = useState<Step>('select')
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)

  const [utxos, setUtxos] = useState<UtxoBox[]>([])
  const [loadingUtxos, setLoadingUtxos] = useState(false)
  const [selectedBoxIds, setSelectedBoxIds] = useState<Set<string>>(new Set())
  const [boardFilter, setBoardFilter] = useState<BoardFilter>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [sortKey, setSortKey] = useState<SortKey>('value-desc')
  const [showHowItWorks, setShowHowItWorks] = useState(false)
  const [consolidateSummary, setConsolidateSummary] = useState<ConsolidateBuildResponse | null>(null)

  const [splitType, setSplitType] = useState<SplitType>('erg')
  const [splitAmount, setSplitAmount] = useState('')
  const [splitCount, setSplitCount] = useState('')
  const [splitTokenId, setSplitTokenId] = useState('')
  const [splitErgPerBox, setSplitErgPerBox] = useState('0.001')
  const [splitSummary, setSplitSummary] = useState<SplitBuildResponse | null>(null)

  const [restructureSlots, setRestructureSlots] = useState<RestructureSlot[]>(() => [
    newRestructureSlot(),
    newRestructureSlot(),
  ])
  const [restructureSummary, setRestructureSummary] = useState<RestructureBuildResponse | null>(null)
  /** Token pool: click-to-select, then click an output (or drag onto it). */
  const [poolSelectedTokenId, setPoolSelectedTokenId] = useState<string | null>(null)
  const [poolAssignAmount, setPoolAssignAmount] = useState('')
  const [draggingTokenId, setDraggingTokenId] = useState<string | null>(null)
  const [dropTargetSlotId, setDropTargetSlotId] = useState<string | null>(null)
  /** Output highlighted for bulk assign (Add all / Add remaining). */
  const [activeOutputSlotId, setActiveOutputSlotId] = useState<string | null>(null)

  const [resolvedNames, setResolvedNames] = useState<Map<string, string>>(new Map())

  const tokens = walletBalance?.tokens ?? []
  const rootClass = embedded ? 'utxo-tab utxo-tab--embedded' : 'utxo-tab'

  useEffect(() => {
    const unknown = tokens.filter(t => !t.name)
    if (unknown.length === 0) return
    let cancelled = false
    for (const t of unknown) {
      getCachedTokenInfo(t.token_id)
        .then(info => {
          if (cancelled) return
          if (info.name) {
            setResolvedNames(prev => {
              const next = new Map(prev)
              next.set(t.token_id, info.name!)
              return next
            })
          }
        })
        .catch(() => {})
    }
    return () => { cancelled = true }
  }, [tokens])

  const getTokenName = useCallback(
    (tokenId: string, name: string | null): string =>
      name || resolvedNames.get(tokenId) || tokenId.slice(0, 8) + '...',
    [resolvedNames],
  )

  const fetchUtxos = useCallback(async () => {
    if (!walletAddress) return
    setLoadingUtxos(true)
    try {
      const raw = await invoke<UtxoBox[]>('get_user_utxos')
      setUtxos(raw)
    } catch (e) {
      console.error('Failed to fetch UTXOs:', e)
    } finally {
      setLoadingUtxos(false)
    }
  }, [walletAddress])

  useEffect(() => {
    if (step === 'select') {
      fetchUtxos()
    }
  }, [subTab, step, fetchUtxos])

  useEffect(() => {
    handleReset()
  }, [walletAddress])

  useEffect(() => {
    if (step !== 'signing' || !requestId) return
    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await getUtxoMgmtTxStatus(requestId)
        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setStep('success')
        } else if (status.status === 'failed' || status.status === 'expired') {
          setError(status.error || 'Transaction failed')
          setStep('error')
        }
      } catch (e) {
        console.error('Poll error:', e)
      } finally {
        isPolling = false
      }
    }
    const interval = setInterval(poll, 2000)
    return () => clearInterval(interval)
  }, [step, requestId])

  const filterCounts = useMemo(() => {
    let dust = 0
    let erg = 0
    let withTokens = 0
    let nfts = 0
    let large = 0
    for (const u of utxos) {
      const nano = ergNano(u)
      if (nano < DUST_NANO) dust++
      if (u.assets.length === 0) erg++
      if (boxHasFungible(u, tokens)) withTokens++
      if (boxHasNft(u, tokens)) nfts++
      if (nano > LARGE_NANO) large++
    }
    return { all: utxos.length, dust, erg, tokens: withTokens, nfts, large }
  }, [utxos, tokens])

  const boardStats = useMemo(() => {
    const totalErg = utxos.reduce((s, u) => s + ergNano(u), 0)
    return { totalErg, count: utxos.length }
  }, [utxos])

  const filteredUtxos = useMemo(() => {
    const q = searchQuery.trim().toLowerCase()
    let list = utxos.filter(u => matchesFilter(u, boardFilter, tokens))
    if (q) {
      list = list.filter(u => {
        if (u.boxId.toLowerCase().includes(q)) return true
        if (String(u.creationHeight).includes(q)) return true
        return u.assets.some(a => a.tokenId.toLowerCase().includes(q))
      })
    }
    const sorted = [...list]
    sorted.sort((a, b) => {
      switch (sortKey) {
        case 'value-asc':
          return ergNano(a) - ergNano(b)
        case 'height-desc':
          return b.creationHeight - a.creationHeight
        case 'height-asc':
          return a.creationHeight - b.creationHeight
        case 'value-desc':
        default:
          return ergNano(b) - ergNano(a)
      }
    })
    return sorted
  }, [utxos, boardFilter, searchQuery, sortKey, tokens])

  const toggleUtxo = (boxId: string) => {
    setSelectedBoxIds(prev => {
      const next = new Set(prev)
      if (next.has(boxId)) next.delete(boxId)
      else next.add(boxId)
      return next
    })
  }

  /** Split mode: single-select source box; click again to clear. */
  const selectSplitSource = (boxId: string) => {
    if (selectedBoxIds.has(boxId) && selectedBoxIds.size === 1) {
      setSelectedBoxIds(new Set())
      return
    }
    setSelectedBoxIds(new Set([boxId]))
    const box = utxos.find(u => u.boxId === boxId)
    if (!box) return
    if (box.assets.length > 0) {
      setSplitType('token')
      setSplitTokenId(box.assets[0].tokenId)
      setSplitAmount('')
      setSplitCount('')
    } else {
      setSplitType('erg')
      setSplitTokenId('')
      setSplitAmount('')
      setSplitCount('')
    }
  }

  const selectVisible = () => {
    setSelectedBoxIds(new Set(filteredUtxos.map(u => u.boxId)))
  }

  const selectAll = () => {
    setSelectedBoxIds(new Set(utxos.map(u => u.boxId)))
  }

  const deselectAllUtxos = () => {
    setSelectedBoxIds(new Set())
  }

  const selectedSplitBox = useMemo(() => {
    if (subTab !== 'split' || selectedBoxIds.size !== 1) return null
    const id = selectedBoxIds.values().next().value as string | undefined
    return id ? utxos.find(u => u.boxId === id) ?? null : null
  }, [subTab, selectedBoxIds, utxos])

  const splitSourceTokens = useMemo(() => {
    if (!selectedSplitBox) return []
    return selectedSplitBox.assets.map(a => {
      const walletToken = tokens.find(t => t.token_id === a.tokenId)
      return {
        token_id: a.tokenId,
        amount: parseInt(a.amount, 10) || 0,
        name: walletToken?.name ?? null,
        decimals: walletToken?.decimals ?? 0,
      }
    })
  }, [selectedSplitBox, tokens])

  const handleConsolidate = async () => {
    setLoading(true)
    setError(null)
    setStep('building')
    try {
      const selectedBoxes = utxos.filter(u => selectedBoxIds.has(u.boxId))
      const userErgoTree = selectedBoxes[0]?.ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const result = await buildConsolidateTx(
        selectedBoxes as unknown as object[],
        userErgoTree,
        nodeStatus.chain_height,
      )
      setConsolidateSummary(result)

      const message = `Consolidate ${selectedBoxes.length} UTXOs`
      const signResult = await startUtxoMgmtSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const splitAmountNano = useMemo(() => {
    if (splitType === 'erg') {
      const parsed = parseFloat(splitAmount.replace(/,/g, ''))
      return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
    }
    return parseInt(splitAmount.replace(/,/g, ''), 10) || 0
  }, [splitType, splitAmount])

  const splitCountNum = useMemo(() => parseInt(splitCount, 10) || 0, [splitCount])

  const splitErgPerBoxNano = useMemo(() => {
    const parsed = parseFloat(splitErgPerBox.replace(/,/g, ''))
    return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
  }, [splitErgPerBox])

  const splitIsValid = useMemo(() => {
    if (!selectedSplitBox) return false
    if (splitCountNum < 1 || splitCountNum > 30) return false
    const boxErg = ergNano(selectedSplitBox)
    if (splitType === 'erg') {
      if (splitAmountNano < MIN_BOX_VALUE_NANO) return false
      const need = splitAmountNano * splitCountNum + WALLET_TX_FEES_NANO
      return boxErg >= need
    }
    if (splitTokenId === '' || splitErgPerBoxNano < MIN_BOX_VALUE_NANO) return false
    const token =
      splitSourceTokens.find(t => t.token_id === splitTokenId) ||
      tokens.find(t => t.token_id === splitTokenId)
    const decimals = token?.decimals ?? 0
    const parsed = parseFloat(splitAmount.replace(/,/g, ''))
    const rawPerBox = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, decimals))
    if (rawPerBox <= 0) return false
    const asset = selectedSplitBox.assets.find(a => a.tokenId === splitTokenId)
    const haveTokens = asset ? parseInt(asset.amount, 10) || 0 : 0
    const needErg = splitErgPerBoxNano * splitCountNum + WALLET_TX_FEES_NANO
    return haveTokens >= rawPerBox * splitCountNum && boxErg >= needErg
  }, [
    selectedSplitBox,
    splitType,
    splitAmountNano,
    splitCountNum,
    splitTokenId,
    splitErgPerBoxNano,
    splitAmount,
    splitSourceTokens,
    tokens,
  ])

  const splitTotalDisplay = useMemo(() => {
    if (splitType === 'erg') {
      return formatErg(splitAmountNano * splitCountNum) + ' ERG'
    }
    const token =
      splitSourceTokens.find(t => t.token_id === splitTokenId) ||
      tokens.find(t => t.token_id === splitTokenId)
    if (!token) return ''
    const decimals = token.decimals
    const parsed = parseFloat(splitAmount.replace(/,/g, ''))
    const rawPerBox = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, decimals))
    return formatTokenAmount(rawPerBox * splitCountNum, decimals)
  }, [splitType, splitAmountNano, splitCountNum, splitTokenId, tokens, splitSourceTokens, splitAmount])

  const handleSplit = async () => {
    setLoading(true)
    setError(null)
    setStep('building')
    try {
      if (!selectedSplitBox) throw new Error('Select a box to split')

      const userErgoTree = selectedSplitBox.ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      let amountStr: string
      if (splitType === 'erg') {
        amountStr = splitAmountNano.toString()
      } else {
        const token =
          splitSourceTokens.find(t => t.token_id === splitTokenId) ||
          tokens.find(t => t.token_id === splitTokenId)
        const decimals = token?.decimals ?? 0
        const parsed = parseFloat(splitAmount.replace(/,/g, ''))
        const raw = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, decimals))
        amountStr = raw.toString()
      }

      const result = await buildSplitTx(
        [selectedSplitBox] as unknown as object[],
        userErgoTree,
        nodeStatus.chain_height,
        splitType,
        amountStr,
        splitCountNum,
        splitType === 'token' ? splitTokenId : undefined,
        splitType === 'token' ? splitErgPerBoxNano : undefined,
      )
      setSplitSummary(result)

      const noun = splitType === 'erg' ? 'ERG' : 'token'
      const message = `Split into ${splitCountNum} ${noun} boxes`
      const signResult = await startUtxoMgmtSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  const restructureInputTokens = useMemo(() => {
    if (subTab !== 'restructure') return [] as Array<{
      token_id: string
      amount: number
      name: string | null
      decimals: number
    }>
    const map = new Map<string, number>()
    for (const u of utxos) {
      if (!selectedBoxIds.has(u.boxId)) continue
      for (const a of u.assets) {
        map.set(a.tokenId, (map.get(a.tokenId) ?? 0) + (parseInt(a.amount, 10) || 0))
      }
    }
    return [...map.entries()].map(([tokenId, amount]) => {
      const wt = tokens.find(t => t.token_id === tokenId)
      return {
        token_id: tokenId,
        amount,
        name: wt?.name ?? null,
        decimals: wt?.decimals ?? 0,
      }
    })
  }, [subTab, utxos, selectedBoxIds, tokens])

  const restructureAssignedRaw = useMemo(() => {
    const map = new Map<string, number>()
    for (const slot of restructureSlots) {
      for (const t of slot.tokens) {
        const meta = restructureInputTokens.find(x => x.token_id === t.tokenId)
          || tokens.find(x => x.token_id === t.tokenId)
        const decimals = meta?.decimals ?? 0
        const parsed = parseFloat(t.amount.replace(/,/g, ''))
        const raw = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, decimals))
        map.set(t.tokenId, (map.get(t.tokenId) ?? 0) + raw)
      }
    }
    return map
  }, [restructureSlots, restructureInputTokens, tokens])

  const restructureRemainingTokens = useMemo(() => {
    return restructureInputTokens
      .map(t => {
        const assigned = restructureAssignedRaw.get(t.token_id) ?? 0
        return { ...t, remaining: t.amount - assigned }
      })
      .filter(t => t.remaining > 0)
  }, [restructureInputTokens, restructureAssignedRaw])

  const restructureAllocatedNano = useMemo(() => {
    return restructureSlots.reduce((sum, s) => {
      const parsed = parseFloat(s.erg.replace(/,/g, ''))
      if (isNaN(parsed) || parsed <= 0) return sum
      return sum + Math.floor(parsed * 1e9)
    }, 0)
  }, [restructureSlots])

  const restructureAvailableNano = useMemo(() => {
    if (subTab !== 'restructure') return 0
    const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
    const total = selected.reduce((s, u) => s + ergNano(u), 0)
    return Math.max(0, total - WALLET_TX_FEES_NANO)
  }, [subTab, utxos, selectedBoxIds])

  const restructureChangeNano = restructureAvailableNano - restructureAllocatedNano

  const restructureErgStatus = useMemo(() => {
    if (subTab !== 'restructure' || selectedBoxIds.size < 1) return null
    const underMin = restructureSlots.some(s => {
      const n = parseErgToNano(s.erg)
      return n === null || n < MIN_BOX_VALUE_NANO
    })
    if (restructureAllocatedNano > restructureAvailableNano) {
      return {
        kind: 'over' as const,
        message: `Over-allocated by ${formatErg(restructureAllocatedNano - restructureAvailableNano)} ERG (fee already reserved)`,
      }
    }
    if (underMin) {
      return {
        kind: 'under-min' as const,
        message: `Each output needs ≥ ${formatErg(MIN_BOX_VALUE_NANO)} ERG`,
      }
    }
    if (restructureChangeNano > 0 && restructureChangeNano < MIN_BOX_VALUE_NANO) {
      return {
        kind: 'dust-change' as const,
        message: `Leftover ${formatErg(restructureChangeNano)} ERG is below min box value — adjust outputs`,
      }
    }
    if (restructureChangeNano === 0) {
      return {
        kind: 'balanced' as const,
        message: `Balanced · ${formatErg(restructureAvailableNano)} ERG after ${formatErg(WALLET_TX_FEES_NANO)} fees`,
      }
    }
    return {
      kind: 'under' as const,
      message: `${formatErg(restructureChangeNano)} ERG unassigned → change box`,
    }
  }, [
    subTab,
    selectedBoxIds.size,
    restructureSlots,
    restructureAllocatedNano,
    restructureAvailableNano,
    restructureChangeNano,
  ])

  const restructureIsValid = useMemo(() => {
    if (subTab !== 'restructure') return false
    if (selectedBoxIds.size < 1) return false
    if (restructureSlots.length < 1 || restructureSlots.length > MAX_RESTRUCTURE_OUTPUTS) return false
    for (const slot of restructureSlots) {
      const parsed = parseFloat(slot.erg.replace(/,/g, ''))
      if (isNaN(parsed) || Math.floor(parsed * 1e9) < MIN_BOX_VALUE_NANO) return false
      for (const t of slot.tokens) {
        const meta = restructureInputTokens.find(x => x.token_id === t.tokenId)
        const decimals = meta?.decimals ?? 0
        const amt = parseFloat(t.amount.replace(/,/g, ''))
        if (isNaN(amt) || Math.floor(amt * Math.pow(10, decimals)) <= 0) return false
      }
      if (slot.tokens.length > 255) return false
    }
    if (restructureAllocatedNano > restructureAvailableNano) return false
    if (restructureChangeNano > 0 && restructureChangeNano < MIN_BOX_VALUE_NANO) return false
    // Unassigned tokens need change ERG
    if (restructureRemainingTokens.length > 0 && restructureChangeNano < MIN_BOX_VALUE_NANO) {
      return false
    }
    // Over-assigned / orphan tokens
    for (const [tid, assigned] of restructureAssignedRaw) {
      const avail = restructureInputTokens.find(t => t.token_id === tid)?.amount ?? 0
      if (assigned > avail) return false
    }
    return true
  }, [
    subTab,
    selectedBoxIds,
    restructureSlots,
    restructureAllocatedNano,
    restructureAvailableNano,
    restructureChangeNano,
    restructureRemainingTokens,
    restructureInputTokens,
    restructureAssignedRaw,
  ])

  const initRestructureSlots = useCallback((selectedErgNano: number) => {
    const available = Math.max(0, selectedErgNano - WALLET_TX_FEES_NANO)
    if (available < MIN_BOX_VALUE_NANO * 2) {
      setRestructureSlots([newRestructureSlot(nanoToErgInput(available))])
      return
    }
    const half = Math.floor(available / 2)
    const other = available - half
    setRestructureSlots([
      newRestructureSlot(nanoToErgInput(half)),
      newRestructureSlot(nanoToErgInput(other)),
    ])
    setPoolSelectedTokenId(null)
    setPoolAssignAmount('')
    setDraggingTokenId(null)
    setDropTargetSlotId(null)
    setActiveOutputSlotId(null)
  }, [])

  // When restructure selection changes, seed default ERG split if slots empty/blank
  useEffect(() => {
    if (subTab !== 'restructure') return
    const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
    if (selected.length === 0) return
    const allBlank = restructureSlots.every(s => !s.erg.trim())
    if (allBlank) {
      const total = selected.reduce((s, u) => s + ergNano(u), 0)
      initRestructureSlots(total)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only seed when selection size changes
  }, [subTab, selectedBoxIds.size])

  // Drop stale active output if the slot was removed
  useEffect(() => {
    if (!activeOutputSlotId) return
    if (!restructureSlots.some(s => s.id === activeOutputSlotId)) {
      setActiveOutputSlotId(null)
    }
  }, [restructureSlots, activeOutputSlotId])

  const addRestructureSlot = () => {
    if (restructureSlots.length >= MAX_RESTRUCTURE_OUTPUTS) return
    setRestructureSlots(prev => {
      const next = [...prev, newRestructureSlot(nanoToErgInput(MIN_BOX_VALUE_NANO))]
      // Pin the new slot at min; last other absorbs remainder
      const newId = next[next.length - 1].id
      return redistributeErg(next, newId, restructureAvailableNano)
    })
  }

  const removeRestructureSlot = (id: string) => {
    setRestructureSlots(prev => {
      if (prev.length <= 1) return prev
      const next = prev.filter(s => s.id !== id)
      // Pin first remaining; last output absorbs freed ERG
      return redistributeErg(next, next[0].id, restructureAvailableNano)
    })
    if (activeOutputSlotId === id) setActiveOutputSlotId(null)
  }

  const updateRestructureErg = (id: string, erg: string) => {
    setRestructureSlots(prev => {
      const withEdit = prev.map(s => (s.id === id ? { ...s, erg } : s))
      // Only auto-balance when the field parses; keep typing intermediate states
      if (parseErgToNano(erg) === null && erg.trim() !== '' && erg.trim() !== '0') {
        // Allow partial input like "20." without clobbering siblings yet
        if (/^\d*\.?\d*$/.test(erg.trim().replace(/,/g, ''))) return withEdit
      }
      if (parseErgToNano(erg) === null) return withEdit
      return redistributeErg(withEdit, id, restructureAvailableNano)
    })
  }

  const removeSlotToken = (slotId: string, tokenId: string) => {
    setRestructureSlots(prev =>
      prev.map(s =>
        s.id === slotId ? { ...s, tokens: s.tokens.filter(t => t.tokenId !== tokenId) } : s,
      ),
    )
  }

  const selectPoolToken = (tokenId: string) => {
    if (poolSelectedTokenId === tokenId) {
      setPoolSelectedTokenId(null)
      setPoolAssignAmount('')
      return
    }
    const rem = restructureRemainingTokens.find(t => t.token_id === tokenId)
    if (!rem || rem.remaining <= 0) return
    setPoolSelectedTokenId(tokenId)
    setPoolAssignAmount(rawToDisplayAmount(rem.remaining, rem.decimals))
  }

  const assignTokenToSlotById = useCallback(
    (tokenId: string, slotId: string, amountDisplay: string) => {
      const meta =
        restructureRemainingTokens.find(t => t.token_id === tokenId) ||
        restructureInputTokens.find(t => t.token_id === tokenId)
      if (!meta) return
      const decimals = meta.decimals
      const parsed = parseFloat(amountDisplay.replace(/,/g, ''))
      if (isNaN(parsed) || parsed <= 0) return
      const raw = Math.floor(parsed * Math.pow(10, decimals))
      const remaining =
        restructureRemainingTokens.find(t => t.token_id === tokenId)?.remaining ?? meta.amount
      if (raw <= 0 || raw > remaining) return

      setRestructureSlots(prev =>
        prev.map(s => {
          if (s.id !== slotId) return s
          const existing = s.tokens.find(t => t.tokenId === tokenId)
          if (existing) {
            const prevParsed = parseFloat(existing.amount.replace(/,/g, '')) || 0
            const nextAmt = rawToDisplayAmount(
              Math.floor(prevParsed * Math.pow(10, decimals)) + raw,
              decimals,
            )
            return {
              ...s,
              tokens: s.tokens.map(t =>
                t.tokenId === tokenId ? { ...t, amount: nextAmt } : t,
              ),
            }
          }
          return {
            ...s,
            tokens: [...s.tokens, { tokenId, amount: rawToDisplayAmount(raw, decimals) }],
          }
        }),
      )

      // Refresh selection amount to leftover (or clear if done)
      const left = remaining - raw
      if (left <= 0) {
        setPoolSelectedTokenId(null)
        setPoolAssignAmount('')
      } else {
        setPoolAssignAmount(rawToDisplayAmount(left, decimals))
      }
    },
    [restructureRemainingTokens, restructureInputTokens],
  )

  const handleOutputSlotClick = (slotId: string) => {
    setActiveOutputSlotId(slotId)
    if (!poolSelectedTokenId || !poolAssignAmount) return
    assignTokenToSlotById(poolSelectedTokenId, slotId, poolAssignAmount)
  }

  /** Put every input token (full amounts) on the active output; clears other slots' tokens. */
  const addAllTokensToActive = () => {
    if (!activeOutputSlotId || restructureInputTokens.length === 0) return
    const targetId = activeOutputSlotId
    const assignments: RestructureTokenAssign[] = restructureInputTokens.map(t => ({
      tokenId: t.token_id,
      amount: rawToDisplayAmount(t.amount, t.decimals),
    }))
    setRestructureSlots(prev =>
      prev.map(s => {
        if (s.id === targetId) return { ...s, tokens: assignments }
        return { ...s, tokens: [] }
      }),
    )
    setPoolSelectedTokenId(null)
    setPoolAssignAmount('')
  }

  /** Assign only unassigned pool remainders onto the active output. */
  const addRemainingTokensToActive = () => {
    if (!activeOutputSlotId || restructureRemainingTokens.length === 0) return
    const targetId = activeOutputSlotId
    const toAdd = restructureRemainingTokens
    setRestructureSlots(prev =>
      prev.map(s => {
        if (s.id !== targetId) return s
        let tokens = [...s.tokens]
        for (const rem of toAdd) {
          const existing = tokens.find(t => t.tokenId === rem.token_id)
          if (existing) {
            const prevParsed = parseFloat(existing.amount.replace(/,/g, '')) || 0
            const nextAmt = rawToDisplayAmount(
              Math.floor(prevParsed * Math.pow(10, rem.decimals)) + rem.remaining,
              rem.decimals,
            )
            tokens = tokens.map(t =>
              t.tokenId === rem.token_id ? { ...t, amount: nextAmt } : t,
            )
          } else {
            tokens = [
              ...tokens,
              {
                tokenId: rem.token_id,
                amount: rawToDisplayAmount(rem.remaining, rem.decimals),
              },
            ]
          }
        }
        return { ...s, tokens }
      }),
    )
    setPoolSelectedTokenId(null)
    setPoolAssignAmount('')
  }

  const evenSplitErg = () => {
    const n = restructureSlots.length
    if (n < 1 || restructureAvailableNano < MIN_BOX_VALUE_NANO * n) return
    const each = Math.floor(restructureAvailableNano / n)
    const last = restructureAvailableNano - each * (n - 1)
    setRestructureSlots(prev =>
      prev.map((s, i) => ({
        ...s,
        erg: nanoToErgInput(i === n - 1 ? last : each),
      })),
    )
  }

  const handleRestructure = async () => {
    setLoading(true)
    setError(null)
    setStep('building')
    try {
      const selectedBoxes = utxos.filter(u => selectedBoxIds.has(u.boxId))
      if (selectedBoxes.length < 1) throw new Error('Select at least one UTXO')
      const userErgoTree = selectedBoxes[0]?.ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const outputs = restructureSlots.map(slot => {
        const parsed = parseFloat(slot.erg.replace(/,/g, ''))
        const value = Math.floor(parsed * 1e9)
        const slotTokens = slot.tokens.map(t => {
          const meta = restructureInputTokens.find(x => x.token_id === t.tokenId)
            || tokens.find(x => x.token_id === t.tokenId)
          const decimals = meta?.decimals ?? 0
          const amt = parseFloat(t.amount.replace(/,/g, ''))
          const raw = Math.floor(amt * Math.pow(10, decimals))
          return { tokenId: t.tokenId, amount: raw.toString() }
        })
        return { value, tokens: slotTokens }
      })

      const result = await buildRestructureTx(
        selectedBoxes as unknown as object[],
        outputs,
        userErgoTree,
        nodeStatus.chain_height,
      )
      setRestructureSummary(result)

      const message = `Restructure ${selectedBoxes.length} → ${result.outputCount} boxes`
      const signResult = await startUtxoMgmtSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
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
      setError(String(e))
    }
  }

  const handleReset = () => {
    setStep('select')
    setError(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setRequestId(null)
    setTxId(null)
    setSignMethod('choose')
    setConsolidateSummary(null)
    setSplitSummary(null)
    setRestructureSummary(null)
    setSelectedBoxIds(new Set())
    setSplitAmount('')
    setSplitCount('')
    setSplitTokenId('')
    setSplitErgPerBox('0.001')
    setRestructureSlots([newRestructureSlot(), newRestructureSlot()])
    setPoolSelectedTokenId(null)
    setPoolAssignAmount('')
    setDraggingTokenId(null)
    setDropTargetSlotId(null)
    setActiveOutputSlotId(null)
    setSearchQuery('')
    setBoardFilter('all')
  }

  const pageHeader = embedded ? null : (
    <header className="utxo-header-main">
      <div className="utxo-header-left">
        <div className="utxo-icon" aria-hidden>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="18" height="18">
            <rect x="3" y="3" width="7" height="7" />
            <rect x="14" y="3" width="7" height="7" />
            <rect x="3" y="14" width="7" height="7" />
            <rect x="14" y="14" width="7" height="7" />
          </svg>
        </div>
        <div>
          <h1 className="utxo-title">UTXO Management</h1>
          <p className="utxo-subtitle">Consolidate, split, or restructure boxes for better UTXO hygiene</p>
        </div>
      </div>
    </header>
  )

  if (!isConnected || !walletAddress) {
    return (
      <div className={rootClass}>
        {pageHeader}
        <EmptyState
          title={!isConnected ? 'Node Required' : 'Wallet Required'}
          description={!isConnected ? 'Connect to a node to manage UTXOs.' : 'Connect your wallet to manage UTXOs.'}
        />
      </div>
    )
  }

  if (step === 'building') {
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="swap-preview-loading">
                <div className="spinner-small" />
                <span>Building transaction...</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'signing' && signMethod === 'choose') {
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                {consolidateSummary && (
                  <div className="utxo-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="utxo-confirm-row">
                      <span>Consolidating</span>
                      <span>{consolidateSummary.inputCount} boxes into 1</span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(consolidateSummary.minerFee)} ERG</span>
                    </div>
                    {consolidateSummary.citadelFeeNano > 0 && (
                      <div className="utxo-confirm-row">
                        <span>Citadel fee</span>
                        <span>{formatErg(consolidateSummary.citadelFeeNano)} ERG</span>
                      </div>
                    )}
                    {consolidateSummary.citadelFeeNano > 0 && (
                      <p className="utxo-muted" style={{ marginTop: 'var(--space-sm)' }}>
                        Includes {formatErg(consolidateSummary.citadelFeeNano)} ERG Citadel fee
                      </p>
                    )}
                  </div>
                )}
                {splitSummary && (
                  <div className="utxo-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="utxo-confirm-row">
                      <span>Splitting into</span>
                      <span>{splitSummary.splitCount} boxes</span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(splitSummary.minerFee)} ERG</span>
                    </div>
                    {splitSummary.citadelFeeNano > 0 && (
                      <div className="utxo-confirm-row">
                        <span>Citadel fee</span>
                        <span>{formatErg(splitSummary.citadelFeeNano)} ERG</span>
                      </div>
                    )}
                    {splitSummary.citadelFeeNano > 0 && (
                      <p className="utxo-muted" style={{ marginTop: 'var(--space-sm)' }}>
                        Includes {formatErg(splitSummary.citadelFeeNano)} ERG Citadel fee
                      </p>
                    )}
                  </div>
                )}
                {restructureSummary && (
                  <div className="utxo-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="utxo-confirm-row">
                      <span>Restructure</span>
                      <span>
                        {restructureSummary.inputCount} → {restructureSummary.outputCount} boxes
                      </span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(restructureSummary.minerFee)} ERG</span>
                    </div>
                    {restructureSummary.citadelFeeNano > 0 && (
                      <div className="utxo-confirm-row">
                        <span>Citadel fee</span>
                        <span>{formatErg(restructureSummary.citadelFeeNano)} ERG</span>
                      </div>
                    )}
                    {restructureSummary.citadelFeeNano > 0 && (
                      <p className="utxo-muted" style={{ marginTop: 'var(--space-sm)' }}>
                        Includes {formatErg(restructureSummary.citadelFeeNano)} ERG Citadel fee
                      </p>
                    )}
                  </div>
                )}
                <p>Choose your signing method</p>
                <div className="wallet-options">
                  <button className="wallet-option" onClick={handleNautilusSign}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="2" y="3" width="20" height="14" rx="2" />
                        <path d="M8 21h8" /><path d="M12 17v4" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Nautilus Extension</span>
                      <span className="wallet-option-desc">Sign with browser extension</span>
                    </div>
                  </button>
                  <button className="wallet-option" onClick={() => setSignMethod('mobile')}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="5" y="2" width="14" height="20" rx="2" />
                        <line x1="12" y1="18" x2="12.01" y2="18" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Mobile Wallet</span>
                      <span className="wallet-option-desc">Scan QR code with Ergo Wallet</span>
                    </div>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'signing' && signMethod === 'nautilus') {
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Approve the transaction in Nautilus</p>
                <div className="nautilus-waiting">
                  <div className="nautilus-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" /><path d="M12 17v4" />
                    </svg>
                  </div>
                  <p className="signing-hint">Waiting for Nautilus approval...</p>
                </div>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
                  <button className="btn btn-primary" onClick={handleNautilusSign}>Open Nautilus Again</button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'signing' && signMethod === 'mobile' && qrUrl) {
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Scan with your Ergo wallet to sign</p>
                <div className="qr-container">
                  <QRCodeSVG value={qrUrl} size={200} />
                </div>
                <p className="signing-hint">Waiting for signature...</p>
                <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'success') {
    const action =
      subTab === 'consolidate' ? 'Consolidated' : subTab === 'split' ? 'Split' : 'Restructured'
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="success-step">
                <div className="success-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                  </svg>
                </div>
                <h3>UTXOs {action}!</h3>
                {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
                <button className="btn btn-primary" onClick={handleReset}>Done</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'error') {
    return (
      <div className={rootClass}>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="error-step">
                <div className="error-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                  </svg>
                </div>
                <h3>Transaction Failed</h3>
                <p className="error-message">{error}</p>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={handleReset}>Start Over</button>
                  <button className="btn btn-primary" onClick={() => { setStep('select'); setError(null) }}>
                    Try Again
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (step === 'confirm') {
    if (subTab === 'consolidate') {
      const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
      const totalErg = selected.reduce((sum, u) => sum + ergNano(u), 0)
      const tokenSet = new Set<string>()
      for (const u of selected) {
        for (const a of u.assets) tokenSet.add(a.tokenId)
      }
      return (
        <div className={rootClass}>
          <div className="utxo-header">
            <h2>Confirm Consolidation</h2>
          </div>
          <div className="utxo-centered-card">
            <div className="card">
              <div className="card-content">
                <div className="utxo-confirm-summary">
                  <div className="utxo-confirm-row">
                    <span>Inputs</span>
                    <span>{selected.length} boxes</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Total ERG</span>
                    <span>{formatErg(totalErg)} ERG</span>
                  </div>
                  {tokenSet.size > 0 && (
                    <div className="utxo-confirm-row">
                      <span>Token types</span>
                      <span>{tokenSet.size}</span>
                    </div>
                  )}
                  <div className="utxo-confirm-row">
                    <span>Miner Fee</span>
                    <span>{formatErg(TX_FEE_NANO)} ERG</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Citadel fee</span>
                    <span>{formatErg(DEV_FEE_NANO)} ERG</span>
                  </div>
                  <p className="utxo-muted">Includes {formatErg(DEV_FEE_NANO)} ERG Citadel fee</p>
                  <div className="utxo-confirm-row utxo-highlight-row">
                    <span>Result</span>
                    <span>1 output box</span>
                  </div>
                </div>

                <div className="utxo-info-box" style={{ marginTop: 'var(--space-md)' }}>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="16" x2="12" y2="12" />
                    <line x1="12" y1="8" x2="12.01" y2="8" />
                  </svg>
                  <p>This will merge {selected.length} boxes into a single UTXO containing all ERG and tokens.</p>
                </div>

                <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                  <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                  <button className="btn btn-primary utxo-action-btn" onClick={handleConsolidate} disabled={loading}>
                    {loading ? 'Building...' : 'Consolidate'}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )
    }

    if (subTab === 'restructure') {
      const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
      return (
        <div className={rootClass}>
          <div className="utxo-header">
            <h2>Confirm Restructure</h2>
          </div>
          <div className="utxo-centered-card">
            <div className="card">
              <div className="card-content">
                <div className="utxo-confirm-summary">
                  <div className="utxo-confirm-row">
                    <span>Inputs</span>
                    <span>{selected.length} boxes</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>User outputs</span>
                    <span>{restructureSlots.length}</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Allocated ERG</span>
                    <span>{formatErg(restructureAllocatedNano)} ERG</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Change / remainder</span>
                    <span>
                      {restructureChangeNano > 0 || restructureRemainingTokens.length > 0
                        ? `${formatErg(Math.max(restructureChangeNano, 0))} ERG`
                          + (restructureRemainingTokens.length > 0
                            ? ` + ${restructureRemainingTokens.length} token type(s)`
                            : '')
                        : 'None'}
                    </span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Miner Fee</span>
                    <span>{formatErg(TX_FEE_NANO)} ERG</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Citadel fee</span>
                    <span>{formatErg(DEV_FEE_NANO)} ERG</span>
                  </div>
                  <p className="utxo-muted">Includes {formatErg(DEV_FEE_NANO)} ERG Citadel fee</p>
                </div>

                <div className="utxo-info-box" style={{ marginTop: 'var(--space-md)' }}>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="16" x2="12" y2="12" />
                    <line x1="12" y1="8" x2="12.01" y2="8" />
                  </svg>
                  <p>
                    Unassigned ERG and tokens after the fee become an automatic change box
                    back to your wallet.
                  </p>
                </div>

                <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                  <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                  <button
                    className="btn btn-primary utxo-action-btn"
                    onClick={handleRestructure}
                    disabled={loading || !restructureIsValid}
                  >
                    {loading ? 'Building...' : 'Restructure'}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )
    }

    return (
      <div className={rootClass}>
        <div className="utxo-header">
          <h2>Confirm Split</h2>
        </div>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="utxo-confirm-summary">
                {selectedSplitBox && (
                  <div className="utxo-confirm-row">
                    <span>Source</span>
                    <span className="mono">
                      {selectedSplitBox.boxId.slice(0, 8)}…{selectedSplitBox.boxId.slice(-6)}
                      {' · '}
                      {formatErg(ergNano(selectedSplitBox))} ERG
                    </span>
                  </div>
                )}
                <div className="utxo-confirm-row">
                  <span>Mode</span>
                  <span>{splitType === 'erg' ? 'ERG Split' : 'Token Split'}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Boxes</span>
                  <span>{splitCountNum}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Per box</span>
                  <span>
                    {splitType === 'erg'
                      ? `${splitAmount} ERG`
                      : (() => {
                          const token =
                            splitSourceTokens.find(t => t.token_id === splitTokenId) ||
                            tokens.find(t => t.token_id === splitTokenId)
                          return `${splitAmount} ${token ? getTokenName(token.token_id, token.name) : 'tokens'}`
                        })()}
                  </span>
                </div>
                <div className="utxo-confirm-row utxo-highlight-row">
                  <span>Total</span>
                  <span>{splitTotalDisplay}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Miner Fee</span>
                  <span>{formatErg(TX_FEE_NANO)} ERG</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Citadel fee</span>
                  <span>{formatErg(DEV_FEE_NANO)} ERG</span>
                </div>
                <p className="utxo-muted">Includes {formatErg(DEV_FEE_NANO)} ERG Citadel fee</p>
              </div>

              <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                <button className="btn btn-primary utxo-action-btn" onClick={handleSplit} disabled={loading || !selectedSplitBox}>
                  {loading ? 'Building...' : 'Split'}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // —— Select step: shared board + mode-specific side panel ——

  const selectedBoxes = utxos.filter(u => selectedBoxIds.has(u.boxId))
  const selectedErg = selectedBoxes.reduce((sum, u) => sum + ergNano(u), 0)
  const selectedTokenMap = new Map<string, number>()
  for (const u of selectedBoxes) {
    for (const a of u.assets) {
      selectedTokenMap.set(a.tokenId, (selectedTokenMap.get(a.tokenId) ?? 0) + (parseInt(a.amount, 10) || 0))
    }
  }
  const resultErg = Math.max(0, selectedErg - WALLET_TX_FEES_NANO)
  const feeFiat = formatFiat(WALLET_TX_FEES_NANO, ergUsdPrice)
  const inputsSaved = Math.max(0, selectedBoxes.length - 1)

  // Primary kind per selected box for donut (NFT > token > ERG)
  let ergShare = 0
  let tokenShare = 0
  let nftShare = 0
  for (const u of selectedBoxes) {
    if (boxHasNft(u, tokens)) nftShare++
    else if (u.assets.length > 0) tokenShare++
    else ergShare++
  }
  const shareSum = Math.max(ergShare + tokenShare + nftShare, 1)
  const donutErg = (ergShare / shareSum) * 100
  const donutToken = (tokenShare / shareSum) * 100
  const donutNft = (nftShare / shareSum) * 100
  const donutStyle = {
    background: selectedBoxes.length === 0
      ? 'conic-gradient(rgba(148,163,184,0.2) 0deg 360deg)'
      : `conic-gradient(
          var(--emerald-500) 0% ${donutErg}%,
          #818cf8 ${donutErg}% ${donutErg + donutToken}%,
          #fbbf24 ${donutErg + donutToken}% ${donutErg + donutToken + donutNft}%
        )`,
  }

  const filterChips: Array<{ id: BoardFilter; label: string; count: number }> = [
    { id: 'all', label: 'All', count: filterCounts.all },
    { id: 'dust', label: 'Dust', count: filterCounts.dust },
    { id: 'erg', label: 'ERG', count: filterCounts.erg },
    { id: 'tokens', label: 'Tokens', count: filterCounts.tokens },
    { id: 'nfts', label: 'NFTs', count: filterCounts.nfts },
  ]

  const renderUtxoCard = (u: UtxoBox, i: number) => {
    const nano = ergNano(u)
    const selected = selectedBoxIds.has(u.boxId)
    const pills = boxPills(u, tokens)
    const fiat = formatFiat(nano, ergUsdPrice)
    return (
      <button
        key={u.boxId}
        type="button"
        role="option"
        aria-selected={selected}
        className={`utxo-card${selected ? ' selected' : ''}`}
        style={{ animationDelay: `${Math.min(i, 24) * 16}ms` }}
        onClick={() =>
          subTab === 'split' ? selectSplitSource(u.boxId) : toggleUtxo(u.boxId)
        }
      >
        <div className="utxo-card-top">
          <span className={`utxo-card-check${selected ? ' on' : ''}`} aria-hidden>
            {selected && (
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                <polyline points="20 6 9 17 4 12" />
              </svg>
            )}
          </span>
        </div>
        <span className="utxo-card-amount mono">
          {formatErg(nano, 2, nano < DUST_NANO ? 4 : 4)} ERG
        </span>
        {fiat && <span className="utxo-card-fiat">{fiat}</span>}
        <code className="utxo-card-id mono">{truncBoxId(u.boxId)}</code>
        <span className="utxo-card-height">
          Block {u.creationHeight.toLocaleString()}
        </span>
        {pills.length > 0 && (
          <div className="utxo-card-pills">
            {pills.map(p => (
              <span key={p} className={`utxo-pill utxo-pill--${p}`}>
                {p === 'dust' ? 'Dust' : p === 'large' ? 'Large' : p === 'token' ? 'Token' : 'NFT'}
              </span>
            ))}
          </div>
        )}
      </button>
    )
  }

  const tokenBreakdown = [...selectedTokenMap.entries()]
    .map(([tokenId, amount]) => {
      const wt = tokens.find(t => t.token_id === tokenId)
      const decimals = wt?.decimals ?? 0
      const name = getTokenName(tokenId, wt?.name ?? null)
      return { tokenId, amount, decimals, name }
    })
    .sort((a, b) => a.name.localeCompare(b.name))

  return (
    <div className={rootClass}>
      {pageHeader}

      <Tabs
        tabs={[
          { id: 'consolidate', label: 'Consolidate' },
          { id: 'split', label: 'Split' },
          { id: 'restructure', label: 'Restructure' },
        ]}
        activeId={subTab}
        onChange={(id) => { setSubTab(id as SubTab); handleReset() }}
        size="compact"
      />

      <div className="utxo-console">
        <div className="utxo-console-main">
          <div className="utxo-page-head">
            <div>
              <h2 className="utxo-page-title">
                {subTab === 'consolidate'
                  ? 'Consolidate UTXOs'
                  : subTab === 'split'
                    ? 'Split UTXO'
                    : 'Restructure UTXOs'}
              </h2>
              <p className="utxo-page-sub">
                {subTab === 'consolidate'
                  ? 'Merge selected boxes into one UTXO to simplify future spends'
                  : subTab === 'split'
                    ? 'Select one box, then choose how to split it'
                    : 'Select inputs, define output boxes, and assign tokens'}
              </p>
            </div>
            {(subTab === 'consolidate' || subTab === 'restructure') && (
              <button
                type="button"
                className="utxo-how-btn"
                onClick={() => setShowHowItWorks(v => !v)}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="12" y1="16" x2="12" y2="12" />
                  <line x1="12" y1="8" x2="12.01" y2="8" />
                </svg>
                How it works
              </button>
            )}
          </div>

          {showHowItWorks && subTab === 'consolidate' && (
            <div className="utxo-info-box utxo-how-panel">
              <p>
                Select two or more UTXOs. Citadel builds a transaction that spends them and creates
                a single output with the combined ERG and tokens (minus the fixed miner fee).
                Sign with Nautilus or ErgoPay.
              </p>
            </div>
          )}

          {showHowItWorks && subTab === 'restructure' && (
            <div className="utxo-info-box utxo-how-panel">
              <p>
                Select inputs, set ERG on outputs (editing one auto-fills the last other so totals
                match after the miner fee), then assign tokens from the pool — click a token then
                an output, or drag onto an output. Select an output and use Add all / Add remaining
                for bulk assign. Leftover tokens return as change.
              </p>
            </div>
          )}

          {(subTab === 'consolidate' || subTab === 'restructure') && (
            <div className="utxo-stats-row">
              <div className="utxo-stat">
                <span className="utxo-stat-label">Total UTXOs</span>
                <span className="utxo-stat-value mono">{boardStats.count}</span>
                <span className="utxo-stat-sub mono">{formatErg(boardStats.totalErg)} ERG</span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">Selected</span>
                <span className="utxo-stat-value mono">{selectedBoxes.length}</span>
                <span className="utxo-stat-sub mono">{formatErg(selectedErg)} ERG</span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">
                  {subTab === 'consolidate' ? 'Est. Result' : 'Outputs'}
                </span>
                <span className="utxo-stat-value mono">
                  {subTab === 'consolidate'
                    ? (selectedBoxes.length >= 2 ? '1' : '—')
                    : (selectedBoxes.length >= 1 ? String(restructureSlots.length) : '—')}
                </span>
                <span className="utxo-stat-sub utxo-stat-sub--accent mono">
                  {subTab === 'consolidate'
                    ? (selectedBoxes.length >= 2 ? `${formatErg(resultErg)} ERG` : '—')
                    : (selectedBoxes.length >= 1
                      ? `${formatErg(restructureAllocatedNano)} allocated`
                      : '—')}
                </span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">Network Fee</span>
                <span className="utxo-stat-value mono">{formatErg(TX_FEE_NANO)}</span>
                <span className="utxo-stat-sub mono">{feeFiat ?? '—'}</span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">
                  {subTab === 'consolidate' ? 'Est. Savings' : 'Change'}
                </span>
                <span className="utxo-stat-value mono">
                  {subTab === 'consolidate'
                    ? '—'
                    : (selectedBoxes.length >= 1 ? formatErg(Math.max(0, restructureChangeNano)) : '—')}
                </span>
                <span className="utxo-stat-sub mono">
                  {subTab === 'consolidate'
                    ? (selectedBoxes.length >= 2 ? `${inputsSaved} inputs` : '—')
                    : (restructureRemainingTokens.length > 0
                      ? `${restructureRemainingTokens.length} token(s) → change`
                      : '—')}
                </span>
              </div>
            </div>
          )}

          {subTab === 'split' && (
            <div className="utxo-stats-row utxo-stats-row--split">
              <div className="utxo-stat">
                <span className="utxo-stat-label">Total UTXOs</span>
                <span className="utxo-stat-value mono">{boardStats.count}</span>
                <span className="utxo-stat-sub mono">{formatErg(boardStats.totalErg)} ERG</span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">Source</span>
                <span className="utxo-stat-value mono">{selectedSplitBox ? '1' : '—'}</span>
                <span className="utxo-stat-sub mono">
                  {selectedSplitBox ? `${formatErg(ergNano(selectedSplitBox))} ERG` : 'Pick a box'}
                </span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">Split into</span>
                <span className="utxo-stat-value mono">{splitCountNum > 0 ? splitCountNum : '—'}</span>
                <span className="utxo-stat-sub utxo-stat-sub--accent mono">
                  {splitIsValid ? splitTotalDisplay : 'Set options →'}
                </span>
              </div>
              <div className="utxo-stat">
                <span className="utxo-stat-label">Network Fee</span>
                <span className="utxo-stat-value mono">{formatErg(TX_FEE_NANO)}</span>
                <span className="utxo-stat-sub mono">{feeFiat ?? '—'}</span>
              </div>
            </div>
          )}

          <div className="utxo-filter-bar">
            <div className="utxo-filter-row">
              <div className="utxo-search">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
                  <circle cx="11" cy="11" r="8" />
                  <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                  type="search"
                  value={searchQuery}
                  onChange={e => setSearchQuery(e.target.value)}
                  placeholder="Search UTXOs by id or token…"
                  aria-label="Search UTXOs"
                />
              </div>
              <div className="utxo-toolbar-actions">
                {(subTab === 'consolidate' || subTab === 'restructure') && (
                  <>
                    <button type="button" className="utxo-toolbar-btn" onClick={selectVisible}>
                      Select visible
                    </button>
                    <button type="button" className="utxo-toolbar-btn" onClick={selectAll}>
                      Select all
                    </button>
                  </>
                )}
                <button type="button" className="utxo-toolbar-btn" onClick={deselectAllUtxos}>
                  Clear
                </button>
                <button
                  type="button"
                  className="utxo-toolbar-btn utxo-toolbar-icon"
                  onClick={fetchUtxos}
                  disabled={loadingUtxos}
                  aria-label="Refresh"
                  title="Refresh"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="23 4 23 10 17 10" />
                    <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                  </svg>
                </button>
              </div>
            </div>

            <div className="utxo-filter-row utxo-filter-row--chips">
              <div className="utxo-board-filters" role="group" aria-label="Filter boxes">
                {filterChips.map(({ id, label, count }) => (
                  <button
                    key={id}
                    type="button"
                    className={`utxo-filter-chip${boardFilter === id ? ' active' : ''}`}
                    onClick={() => setBoardFilter(id)}
                  >
                    {label} ({count})
                  </button>
                ))}
                <button
                  type="button"
                  className={`utxo-filter-chip utxo-filter-chip--soft${boardFilter === 'large' ? ' active' : ''}`}
                  onClick={() => setBoardFilter(boardFilter === 'large' ? 'all' : 'large')}
                >
                  Large &gt; 10 ERG ({filterCounts.large})
                </button>
              </div>
              <label className="utxo-sort">
                <span className="sr-only">Sort</span>
                <select
                  value={sortKey}
                  onChange={e => setSortKey(e.target.value as SortKey)}
                  aria-label="Sort UTXOs"
                >
                  <option value="value-desc">Value: High to Low</option>
                  <option value="value-asc">Value: Low to High</option>
                  <option value="height-desc">Block: Newest</option>
                  <option value="height-asc">Block: Oldest</option>
                </select>
              </label>
            </div>
          </div>

          <div
            className="utxo-board-canvas"
            role="listbox"
            aria-multiselectable={subTab !== 'split'}
            aria-label={
              subTab === 'split'
                ? 'Select a box to split'
                : subTab === 'restructure'
                  ? 'Select UTXOs to restructure'
                  : 'UTXO boxes'
            }
          >
            {loadingUtxos ? (
              <div className="utxo-board-empty">
                <div className="spinner-small" />
                <span>Loading UTXOs…</span>
              </div>
            ) : filteredUtxos.length === 0 ? (
              <div className="utxo-board-empty">
                <span>{utxos.length === 0 ? 'No UTXOs found' : 'No boxes match this filter'}</span>
              </div>
            ) : (
              <div className="utxo-card-grid">
                {filteredUtxos.map((u, i) => renderUtxoCard(u, i))}
              </div>
            )}
          </div>
        </div>

        <aside className="utxo-console-side">
          {subTab === 'consolidate' ? (
            <>
              <section className="utxo-side-section">
                <h3 className="utxo-side-title">Consolidation Preview</h3>
                <div className="utxo-donut-wrap">
                  <div className="utxo-donut" style={donutStyle}>
                    <div className="utxo-donut-hole">
                      <span className="utxo-donut-value mono">
                        {selectedBoxes.length > 0 ? formatErg(selectedErg, 2, 4) : '0'}
                      </span>
                      <span className="utxo-donut-unit">ERG</span>
                    </div>
                  </div>
                  <ul className="utxo-donut-legend">
                    <li><i className="utxo-swatch erg" /> ERG ({ergShare})</li>
                    <li><i className="utxo-swatch token" /> Tokens ({tokenShare})</li>
                    <li><i className="utxo-swatch nft" /> NFTs ({nftShare})</li>
                  </ul>
                </div>
                <div className="utxo-info-box utxo-why-box">
                  <p>
                    Why consolidate? Fewer boxes means simpler coin selection and lower chance of
                    needing many inputs on the next spend.
                  </p>
                </div>
              </section>

              <section className="utxo-side-section">
                <h3 className="utxo-side-title">Token Breakdown (Selected)</h3>
                {tokenBreakdown.length === 0 ? (
                  <p className="utxo-side-empty">No tokens in selection</p>
                ) : (
                  <ul className="utxo-token-list">
                    {tokenBreakdown.map(t => (
                      <li key={t.tokenId}>
                        <span className="utxo-token-avatar" aria-hidden>
                          {t.name.slice(0, 1).toUpperCase()}
                        </span>
                        <div className="utxo-token-meta">
                          <span className="utxo-token-name">{t.name}</span>
                          <span className="utxo-token-id mono">{truncBoxId(t.tokenId)}</span>
                        </div>
                        <span className="utxo-token-amt mono">
                          {formatTokenAmount(t.amount, t.decimals)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              <section className="utxo-side-section">
                <h3 className="utxo-side-title">Consolidation Settings</h3>
                <div className="utxo-settings">
                  <div className="utxo-setting-row">
                    <span>Result</span>
                    <span className="mono">1 UTXO</span>
                  </div>
                  <div className="utxo-setting-row">
                    <span>Network Fee</span>
                    <span className="mono">{formatErg(TX_FEE_NANO)} ERG (fixed)</span>
                  </div>
                  <div className="utxo-setting-row">
                    <span>Change Address</span>
                    <span className="mono utxo-setting-addr" title={walletAddress}>
                      {truncateAddress(walletAddress, 6)}
                    </span>
                  </div>
                </div>
              </section>

              {error && <div className="message error">{error}</div>}

              <button
                type="button"
                className="utxo-submit-btn"
                onClick={() => setStep('confirm')}
                disabled={selectedBoxIds.size < 2}
              >
                {selectedBoxIds.size < 2
                  ? 'Select ≥2 boxes'
                  : 'Preview Consolidation →'}
              </button>
              <p className="utxo-step-hint">Step 1 of 3</p>
            </>
          ) : subTab === 'restructure' ? (
            selectedBoxes.length === 0 ? (
              <div className="utxo-split-empty">
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
                  <rect x="3" y="3" width="7" height="7" />
                  <rect x="14" y="3" width="7" height="7" />
                  <rect x="3" y="14" width="7" height="7" />
                  <rect x="14" y="14" width="7" height="7" />
                </svg>
                <p className="utxo-split-empty-title">Select inputs to restructure</p>
                <p className="utxo-split-empty-hint">Pick one or more UTXO cards</p>
              </div>
            ) : (
              <>
                <section className="utxo-side-section">
                  <div className="utxo-side-title-row">
                    <h3 className="utxo-side-title">Token pool</h3>
                    {poolSelectedTokenId && (
                      <button
                        type="button"
                        className="utxo-toolbar-btn"
                        onClick={() => {
                          setPoolSelectedTokenId(null)
                          setPoolAssignAmount('')
                        }}
                      >
                        Clear
                      </button>
                    )}
                  </div>
                  {restructureInputTokens.length === 0 ? (
                    <p className="utxo-side-empty">No tokens in selection</p>
                  ) : (
                    <>
                      <div className="utxo-token-pool" role="list" aria-label="Unassigned tokens">
                        {restructureInputTokens.map(t => {
                          const assigned = restructureAssignedRaw.get(t.token_id) ?? 0
                          const rem = t.amount - assigned
                          const name = getTokenName(t.token_id, t.name)
                          const selected = poolSelectedTokenId === t.token_id
                          const fully = rem <= 0
                          return (
                            <button
                              key={t.token_id}
                              type="button"
                              role="listitem"
                              draggable={!fully}
                              disabled={fully}
                              className={
                                `utxo-pool-chip` +
                                (selected ? ' selected' : '') +
                                (fully ? ' done' : '') +
                                (draggingTokenId === t.token_id ? ' dragging' : '')
                              }
                              onClick={() => {
                                if (!fully) selectPoolToken(t.token_id)
                              }}
                              onDragStart={e => {
                                if (fully) return
                                setDraggingTokenId(t.token_id)
                                e.dataTransfer.setData('text/token-id', t.token_id)
                                e.dataTransfer.setData('text/plain', t.token_id)
                                e.dataTransfer.effectAllowed = 'move'
                                // Preselect full remaining for drop
                                setPoolSelectedTokenId(t.token_id)
                                setPoolAssignAmount(rawToDisplayAmount(rem, t.decimals))
                              }}
                              onDragEnd={() => {
                                setDraggingTokenId(null)
                                setDropTargetSlotId(null)
                              }}
                              title={
                                fully
                                  ? `${name} fully assigned`
                                  : `Click then click an output, or drag · ${formatTokenAmount(rem, t.decimals)} left`
                              }
                            >
                              <span className="utxo-pool-chip-avatar" aria-hidden>
                                {name.slice(0, 1).toUpperCase()}
                              </span>
                              <span className="utxo-pool-chip-meta">
                                <span className="utxo-pool-chip-name">{name}</span>
                                <span className="utxo-pool-chip-amt mono">
                                  {fully
                                    ? 'assigned'
                                    : `${formatTokenAmount(rem, t.decimals)} left`}
                                </span>
                              </span>
                            </button>
                          )
                        })}
                      </div>
                      <div className="utxo-pool-bulk-bar">
                        <button
                          type="button"
                          className="utxo-toolbar-btn"
                          onClick={addAllTokensToActive}
                          disabled={!activeOutputSlotId || restructureInputTokens.length === 0}
                          title={
                            activeOutputSlotId
                              ? 'Move full amounts of every input token onto the selected output'
                              : 'Select an output first'
                          }
                        >
                          Add all
                        </button>
                        <button
                          type="button"
                          className="utxo-toolbar-btn"
                          onClick={addRemainingTokensToActive}
                          disabled={!activeOutputSlotId || restructureRemainingTokens.length === 0}
                          title={
                            activeOutputSlotId
                              ? 'Assign unassigned pool remainders onto the selected output'
                              : 'Select an output first'
                          }
                        >
                          Add remaining
                        </button>
                        <span className="utxo-pool-bulk-hint">
                          {activeOutputSlotId
                            ? `Target: Out ${
                                restructureSlots.findIndex(s => s.id === activeOutputSlotId) + 1
                              }`
                            : 'Select an output to assign'}
                        </span>
                      </div>
                      {poolSelectedTokenId && (
                        <div className="utxo-pool-assign-bar">
                          <label htmlFor="utxo-pool-amt">Amount</label>
                          <input
                            id="utxo-pool-amt"
                            type="text"
                            inputMode="decimal"
                            value={poolAssignAmount}
                            onChange={e => setPoolAssignAmount(e.target.value)}
                            className="utxo-split-input"
                          />
                          <button
                            type="button"
                            className="utxo-toolbar-btn"
                            onClick={() => {
                              const rem = restructureRemainingTokens.find(
                                t => t.token_id === poolSelectedTokenId,
                              )
                              if (rem) {
                                setPoolAssignAmount(rawToDisplayAmount(rem.remaining, rem.decimals))
                              }
                            }}
                          >
                            Max
                          </button>
                          <span className="utxo-pool-assign-hint">
                            Click or drop on an output
                          </span>
                        </div>
                      )}
                    </>
                  )}
                </section>

                <section className="utxo-side-section">
                  <div className="utxo-side-title-row">
                    <h3 className="utxo-side-title">Output slots</h3>
                    <div className="utxo-side-title-actions">
                      <button type="button" className="utxo-toolbar-btn" onClick={evenSplitErg}>
                        Even ERG
                      </button>
                      <button
                        type="button"
                        className="utxo-toolbar-btn"
                        onClick={addRestructureSlot}
                        disabled={restructureSlots.length >= MAX_RESTRUCTURE_OUTPUTS}
                      >
                        + Add
                      </button>
                    </div>
                  </div>

                  {restructureErgStatus && (
                    <div
                      className={
                        `utxo-erg-status utxo-erg-status--${restructureErgStatus.kind}`
                      }
                      role="status"
                    >
                      {restructureErgStatus.message}
                    </div>
                  )}

                  <div className="utxo-restructure-slots">
                    {restructureSlots.map((slot, idx) => {
                      const isDropTarget = dropTargetSlotId === slot.id
                      const isActiveTarget = activeOutputSlotId === slot.id
                      const isAssignTarget = !!poolSelectedTokenId
                      const slotNano = parseErgToNano(slot.erg)
                      const underMin =
                        slotNano !== null && slotNano > 0 && slotNano < MIN_BOX_VALUE_NANO
                      return (
                        <div
                          key={slot.id}
                          className={
                            `utxo-restructure-slot` +
                            (isDropTarget ? ' drop-hover' : '') +
                            (isAssignTarget ? ' assign-ready' : '') +
                            (isActiveTarget ? ' active-target' : '') +
                            (underMin ? ' under-min' : '')
                          }
                          onClick={e => {
                            // Don't steal clicks from inputs/buttons
                            const tag = (e.target as HTMLElement).closest(
                              'input, button, label, a',
                            )
                            if (tag) return
                            handleOutputSlotClick(slot.id)
                          }}
                          onDragOver={e => {
                            if (!draggingTokenId) return
                            e.preventDefault()
                            e.dataTransfer.dropEffect = 'move'
                            setDropTargetSlotId(slot.id)
                          }}
                          onDragLeave={e => {
                            if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                              setDropTargetSlotId(prev => (prev === slot.id ? null : prev))
                            }
                          }}
                          onDrop={e => {
                            e.preventDefault()
                            const tid =
                              e.dataTransfer.getData('text/token-id') ||
                              e.dataTransfer.getData('text/plain') ||
                              draggingTokenId
                            setDropTargetSlotId(null)
                            setDraggingTokenId(null)
                            setActiveOutputSlotId(slot.id)
                            if (!tid) return
                            const rem = restructureRemainingTokens.find(t => t.token_id === tid)
                            const amt =
                              poolSelectedTokenId === tid && poolAssignAmount
                                ? poolAssignAmount
                                : rem
                                  ? rawToDisplayAmount(rem.remaining, rem.decimals)
                                  : ''
                            if (amt) assignTokenToSlotById(tid, slot.id, amt)
                          }}
                        >
                          <div className="utxo-restructure-slot-head">
                            <span className="utxo-restructure-slot-label">
                              Out {idx + 1}
                              {isActiveTarget ? ' · target' : ''}
                              {!isActiveTarget
                                && idx === restructureSlots.length - 1
                                && restructureSlots.length > 1
                                ? ' · remainder'
                                : ''}
                            </span>
                            {restructureSlots.length > 1 && (
                              <button
                                type="button"
                                className="utxo-restructure-remove"
                                onClick={() => removeRestructureSlot(slot.id)}
                                aria-label={`Remove output ${idx + 1}`}
                              >
                                ×
                              </button>
                            )}
                          </div>
                          <div className="utxo-split-field">
                            <label htmlFor={`re-erg-${slot.id}`}>ERG</label>
                            <input
                              id={`re-erg-${slot.id}`}
                              type="text"
                              inputMode="decimal"
                              value={slot.erg}
                              onChange={e => updateRestructureErg(slot.id, e.target.value)}
                              onBlur={() => {
                                const n = parseErgToNano(slot.erg)
                                if (n !== null) {
                                  updateRestructureErg(slot.id, nanoToErgInput(n))
                                }
                              }}
                              placeholder="1.0"
                              className="utxo-split-input"
                            />
                          </div>
                          <div className="utxo-restructure-dropzone">
                            {slot.tokens.length === 0 ? (
                              <p className="utxo-restructure-drop-hint">
                                {poolSelectedTokenId
                                  ? 'Click to assign selected token'
                                  : isActiveTarget
                                    ? 'Active assign target'
                                    : 'Click to select · drop tokens'}
                              </p>
                            ) : (
                              <ul className="utxo-restructure-token-chips">
                                {slot.tokens.map(t => {
                                  const meta = restructureInputTokens.find(
                                    x => x.token_id === t.tokenId,
                                  )
                                  const name = getTokenName(t.tokenId, meta?.name ?? null)
                                  return (
                                    <li key={t.tokenId}>
                                      <button
                                        type="button"
                                        className="utxo-assigned-chip"
                                        onClick={() => removeSlotToken(slot.id, t.tokenId)}
                                        title={`Return ${name} to pool`}
                                      >
                                        <span>{name}</span>
                                        <span className="mono">{t.amount}</span>
                                        <span className="utxo-assigned-chip-x" aria-hidden>×</span>
                                      </button>
                                    </li>
                                  )
                                })}
                              </ul>
                            )}
                          </div>
                        </div>
                      )
                    })}
                  </div>
                </section>

                <section className="utxo-side-section">
                  <h3 className="utxo-side-title">Balance</h3>
                  <div className="utxo-settings">
                    <div className="utxo-setting-row">
                      <span>Inputs</span>
                      <span className="mono">{formatErg(selectedErg)} ERG</span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Miner fee</span>
                      <span className="mono">{formatErg(TX_FEE_NANO)} ERG</span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Citadel fee</span>
                      <span className="mono">{formatErg(DEV_FEE_NANO)} ERG</span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Spendable</span>
                      <span className="mono">{formatErg(restructureAvailableNano)} ERG</span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Allocated</span>
                      <span className="mono">{formatErg(restructureAllocatedNano)} ERG</span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Change ERG</span>
                      <span className="mono">
                        {restructureChangeNano > 0
                          ? `${formatErg(restructureChangeNano)} ERG`
                          : 'None'}
                      </span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Unassigned tokens</span>
                      <span className="mono">
                        {restructureRemainingTokens.length > 0
                          ? `${restructureRemainingTokens.length} → change`
                          : 'None'}
                      </span>
                    </div>
                    <div className="utxo-setting-row">
                      <span>Change address</span>
                      <span className="mono utxo-setting-addr" title={walletAddress}>
                        {truncateAddress(walletAddress, 6)}
                      </span>
                    </div>
                  </div>
                </section>

                {error && <div className="message error">{error}</div>}

                <button
                  type="button"
                  className="utxo-submit-btn"
                  onClick={() => setStep('confirm')}
                  disabled={!restructureIsValid}
                >
                  {selectedBoxIds.size < 1
                    ? 'Select ≥1 box'
                    : restructureAllocatedNano > restructureAvailableNano
                      ? 'Over-allocated'
                      : !restructureIsValid
                        ? 'Fix allocation'
                        : 'Preview Restructure →'}
                </button>
                <p className="utxo-step-hint">Step 1 of 3</p>
              </>
            )
          ) : !selectedSplitBox ? (
            <div className="utxo-split-empty">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
                <rect x="3" y="3" width="7" height="7" />
                <rect x="14" y="3" width="7" height="7" />
                <rect x="3" y="14" width="7" height="7" />
                <rect x="14" y="14" width="7" height="7" />
              </svg>
              <p className="utxo-split-empty-title">Select a box to split</p>
              <p className="utxo-split-empty-hint">Click one UTXO card on the board</p>
            </div>
          ) : (
            <>
              <div className="utxo-focus-card utxo-split-source-card">
                <code className="mono utxo-focus-id">
                  {selectedSplitBox.boxId.slice(0, 10)}…{selectedSplitBox.boxId.slice(-8)}
                </code>
                <span className="mono">{formatErg(ergNano(selectedSplitBox))} ERG</span>
                {selectedSplitBox.assets.length > 0 && (
                  <span className="utxo-focus-tokens">
                    {selectedSplitBox.assets.length} token type
                    {selectedSplitBox.assets.length !== 1 ? 's' : ''}
                  </span>
                )}
              </div>

              <div className="utxo-split-type-toggle">
                <button
                  type="button"
                  className={`utxo-split-type-btn ${splitType === 'erg' ? 'active' : ''}`}
                  onClick={() => { setSplitType('erg'); setSplitAmount(''); setSplitCount('') }}
                >
                  Split ERG
                </button>
                <button
                  type="button"
                  className={`utxo-split-type-btn ${splitType === 'token' ? 'active' : ''}`}
                  onClick={() => {
                    setSplitType('token')
                    setSplitAmount('')
                    setSplitCount('')
                    if (!splitTokenId && splitSourceTokens[0]) {
                      setSplitTokenId(splitSourceTokens[0].token_id)
                    }
                  }}
                  disabled={splitSourceTokens.length === 0}
                >
                  Split Token
                </button>
              </div>

              <div className="utxo-split-form">
                {splitType === 'token' && (
                  <div className="utxo-split-field">
                    <label>Token</label>
                    <select
                      value={splitTokenId}
                      onChange={e => setSplitTokenId(e.target.value)}
                      className="utxo-split-select"
                    >
                      <option value="">Select token...</option>
                      {splitSourceTokens.map(t => (
                        <option key={t.token_id} value={t.token_id}>
                          {getTokenName(t.token_id, t.name)} ({formatTokenAmount(t.amount, t.decimals)})
                        </option>
                      ))}
                    </select>
                  </div>
                )}

                <div className="utxo-split-field">
                  <label>{splitType === 'erg' ? 'ERG per box' : 'Tokens per box'}</label>
                  <input
                    type="text"
                    inputMode="decimal"
                    value={splitAmount}
                    onChange={e => setSplitAmount(e.target.value)}
                    placeholder={splitType === 'erg' ? '1.0' : '100'}
                    className="utxo-split-input"
                  />
                </div>

                <div className="utxo-split-field">
                  <label>Number of boxes (1–30)</label>
                  <div className="utxo-split-alloc">
                    <input
                      type="range"
                      min={1}
                      max={30}
                      value={Math.min(30, Math.max(1, splitCountNum || 1))}
                      onChange={e => setSplitCount(e.target.value)}
                      className="utxo-split-range"
                      aria-label="Split box count"
                    />
                    <input
                      type="text"
                      inputMode="numeric"
                      value={splitCount}
                      onChange={e => setSplitCount(e.target.value.replace(/\D/g, ''))}
                      placeholder="5"
                      className="utxo-split-input utxo-split-count"
                    />
                  </div>
                </div>

                {splitType === 'token' && (
                  <div className="utxo-split-field">
                    <label>ERG per box</label>
                    <input
                      type="text"
                      inputMode="decimal"
                      value={splitErgPerBox}
                      onChange={e => setSplitErgPerBox(e.target.value)}
                      placeholder="0.001"
                      className="utxo-split-input"
                    />
                  </div>
                )}

                {splitCountNum > 0 && splitAmount && (
                  <div className="utxo-confirm-summary">
                    <div className="utxo-confirm-row">
                      <span>Total</span>
                      <span>{splitTotalDisplay || '—'}</span>
                    </div>
                    {splitType === 'token' && (
                      <div className="utxo-confirm-row">
                        <span>ERG locked</span>
                        <span>{formatErg(splitErgPerBoxNano * splitCountNum)} ERG</span>
                      </div>
                    )}
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(TX_FEE_NANO)} ERG</span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Citadel fee</span>
                      <span>{formatErg(DEV_FEE_NANO)} ERG</span>
                    </div>
                    <p className="utxo-muted">Includes {formatErg(DEV_FEE_NANO)} ERG Citadel fee</p>
                  </div>
                )}
              </div>

              <div className="utxo-split-preview utxo-split-preview--compact">
                <div className="utxo-split-flow">
                  <div className="utxo-split-stage">
                    <div className="utxo-split-preview-label">Before</div>
                    <div className="utxo-ghost-tile utxo-ghost-source kind-large">
                      <span className="utxo-ghost-caption">source</span>
                      <span className="mono utxo-ghost-value">
                        {formatErg(ergNano(selectedSplitBox), 0, 2)}
                      </span>
                      <span className="utxo-ghost-sub">
                        {splitCountNum > 0 ? `${splitCountNum}× →` : 'set count'}
                      </span>
                    </div>
                  </div>

                  <div className="utxo-split-arrow" aria-hidden>→</div>

                  <div className="utxo-split-stage utxo-split-stage-after">
                    <div className="utxo-split-preview-label">After · {splitCountNum || 0} boxes</div>
                    {splitCountNum > 0 && splitIsValid ? (
                      <div className="utxo-split-ghosts">
                        {Array.from({ length: Math.min(splitCountNum, 30) }, (_, i) => (
                          <div
                            key={i}
                            className={`utxo-ghost-tile${splitType === 'token' ? ' token' : ''}`}
                            style={{ animationDelay: `${i * 30}ms` }}
                          >
                            <span className="mono">
                              {splitType === 'erg' ? formatErg(splitAmountNano, 0, 2) : splitAmount}
                            </span>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <p className="utxo-split-preview-hint">
                        {splitCountNum > 0
                          ? 'Adjust amount to fit this box'
                          : 'Set amount & count to preview'}
                      </p>
                    )}
                  </div>
                </div>
              </div>

              {error && <div className="message error">{error}</div>}

              <button
                type="button"
                className="utxo-submit-btn"
                onClick={() => setStep('confirm')}
                disabled={!splitIsValid}
              >
                {!selectedSplitBox
                  ? 'Select a box to split'
                  : !splitIsValid
                    ? 'Set valid split options'
                    : 'Review Split'}
              </button>
            </>
          )}
        </aside>
      </div>
    </div>
  )
}
