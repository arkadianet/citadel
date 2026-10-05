import type { RestructureOutputSpec } from '../../api/utxoManagement'
import type { UtxoBox, WalletToken } from './types'

export interface IsolationToken {
  tokenId: string
  name: string | null
  decimals: number
  rawAmount: string
}

export interface IsolationAllocation {
  tokenId: string
  amount: string
  erg: string
}

export interface IsolationPlan {
  outputs: RestructureOutputSpec[]
  remainingTokens: IsolationToken[]
  allocatedNano: number
  changeNano: number
  fundingNeededNano: number
  /** Alternative funding that balances all outputs exactly, without a change box. */
  exactFundingNano: number | null
  error: string | null
}

/** Parse display units without floating-point rounding or silently truncating fractions. */
export function parseDisplayUnits(value: string, decimals: number): bigint | null {
  const input = value.trim()
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 255) return null
  if (!/^(?:\d+(?:\.\d*)?|\.\d+)$/.test(input)) return null
  const [whole = '', fraction = ''] = input.split('.')
  if (fraction.length > decimals) return null
  return BigInt(whole || '0') * 10n ** BigInt(decimals)
    + BigInt(fraction.padEnd(decimals, '0') || '0')
}

export function displayRawAmount(raw: string, decimals: number): string {
  const amount = BigInt(raw)
  if (decimals === 0) return amount.toString()
  const digits = amount.toString().padStart(decimals + 1, '0')
  const fraction = digits.slice(-decimals).replace(/0+$/, '')
  return digits.slice(0, -decimals) + (fraction ? `.${fraction}` : '')
}

export function collectIsolationTokens(boxes: UtxoBox[], metadata: WalletToken[]): IsolationToken[] {
  const amounts = new Map<string, bigint>()
  for (const box of boxes) {
    for (const asset of box.assets) {
      amounts.set(asset.tokenId, (amounts.get(asset.tokenId) ?? 0n) + BigInt(asset.amount))
    }
  }
  return [...amounts].map(([tokenId, amount]) => {
    const token = metadata.find(t => t.token_id === tokenId)
    return { tokenId, rawAmount: amount.toString(), name: token?.name ?? null, decimals: token?.decimals ?? 0 }
  })
}

export function planTokenIsolation(
  available: IsolationToken[],
  allocations: IsolationAllocation[],
  totalNano: number,
  feeNano: number,
  minBoxNano: number,
  maxOutputs: number,
): IsolationPlan {
  const plan: IsolationPlan = {
    outputs: [], remainingTokens: available, allocatedNano: 0,
    changeNano: totalNano - feeNano, fundingNeededNano: 0, exactFundingNano: null, error: null,
  }
  const invalid = (error: string) => ({ ...plan, error })
  if (allocations.length === 0) return invalid('Select the tokens or NFTs to separate.')
  if (allocations.length > maxOutputs) return invalid(`Separate up to ${maxOutputs} token types at once.`)
  if (!Number.isSafeInteger(totalNano) || totalNano < 0) return invalid('Input ERG amount is too large.')
  const assigned = new Map<string, bigint>()
  for (const allocation of allocations) {
    const token = available.find(t => t.tokenId === allocation.tokenId)
    if (!token) return invalid('A selected token is no longer in the selected inputs. Choose it again.')
    if (assigned.has(token.tokenId)) return invalid('Each token type can only have one separate output.')
    const amount = parseDisplayUnits(allocation.amount, token.decimals)
    if (amount === null || amount <= 0n || amount > BigInt(token.rawAmount) || amount > 9_223_372_036_854_775_807n) {
      return invalid(`Enter a valid amount for ${token.name || token.tokenId.slice(0, 8)} (up to ${displayRawAmount(token.rawAmount, token.decimals)}).`)
    }
    const nano = parseDisplayUnits(allocation.erg, 9)
    if (nano === null || nano < BigInt(minBoxNano) || nano > BigInt(Number.MAX_SAFE_INTEGER)) {
      return invalid(`Each separate box needs at least ${displayRawAmount(String(minBoxNano), 9)} ERG.`)
    }
    assigned.set(token.tokenId, amount)
    plan.outputs.push({ value: Number(nano), tokens: [{ tokenId: token.tokenId, amount: amount.toString() }] })
    plan.allocatedNano += Number(nano)
    if (!Number.isSafeInteger(plan.allocatedNano)) return invalid('Allocated ERG amount is too large.')
  }
  plan.remainingTokens = available.flatMap(token => {
    const remaining = BigInt(token.rawAmount) - (assigned.get(token.tokenId) ?? 0n)
    return remaining > 0n ? [{ ...token, rawAmount: remaining.toString() }] : []
  })
  plan.changeNano = totalNano - feeNano - plan.allocatedNano
  const needsChange = plan.remainingTokens.length > 0 || plan.changeNano !== 0
  plan.fundingNeededNano = Math.max(0, plan.allocatedNano + feeNano + (needsChange ? minBoxNano : 0) - totalNano)
  plan.exactFundingNano = plan.remainingTokens.length === 0 && plan.changeNano < 0
    ? -plan.changeNano : null
  if (plan.fundingNeededNano > 0) {
    plan.error = plan.exactFundingNano !== null
      ? `Add ${displayRawAmount(String(plan.exactFundingNano), 9)} ERG to balance outputs and the miner fee exactly, or at least ${displayRawAmount(String(plan.fundingNeededNano), 9)} ERG to include change.`
      : `Select more ERG inputs: at least ${displayRawAmount(String(plan.fundingNeededNano), 9)} ERG needed for outputs, the miner fee and change.`
  }
  return plan
}

/** Prefer one sufficient ERG-only input; otherwise use the fewest large inputs. */
export function findIsolationFunding(boxes: UtxoBox[], selected: Set<string>, neededNano: number, exactNano: number | null = null): string[] {
  if (neededNano <= 0) return []
  const candidates = boxes.filter(box => box.assets.length === 0 && !selected.has(box.boxId))
    .sort((a, b) => Number(a.value) - Number(b.value))
  const exact = exactNano !== null && candidates.find(box => Number(box.value) === exactNano)
  if (exact) return [exact.boxId]
  const sufficient = candidates.find(box => Number(box.value) >= neededNano)
  if (sufficient) return [sufficient.boxId]
  const result: string[] = []
  let collected = 0
  for (const box of candidates.reverse()) {
    result.push(box.boxId)
    collected += Number(box.value)
    if (collected >= neededNano || collected === exactNano) return result
  }
  return []
}
