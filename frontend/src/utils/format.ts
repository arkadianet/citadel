/**
 * Format a raw token amount for display using its decimal places.
 */
export function formatTokenAmount(
  amount: number,
  decimals: number,
  minDecimals?: number,
  maxDecimals?: number,
): string {
  if (decimals === 0) return amount.toLocaleString()
  const divisor = Math.pow(10, decimals)
  return (amount / divisor).toLocaleString(undefined, {
    minimumFractionDigits: minDecimals ?? decimals,
    maximumFractionDigits: maxDecimals ?? decimals,
  })
}

/**
 * Format nanoERG amount for display.
 * Default: 2-4 decimal places (matches most protocol UIs).
 */
export function formatErg(
  nanoErg: number,
  minDecimals: number = 2,
  maxDecimals: number = 4,
): string {
  return (nanoErg / 1_000_000_000).toLocaleString(undefined, {
    minimumFractionDigits: minDecimals,
    maximumFractionDigits: maxDecimals,
  })
}

/** Truncate an address or hash for display */
export function truncateAddress(addr: string, chars: number = 8): string {
  if (addr.length <= chars * 2 + 3) return addr
  return `${addr.slice(0, chars)}...${addr.slice(-chars)}`
}

/** Format a block count as human-readable duration (2 min/block) */
export function blocksToTime(blocks: number): string {
  const minutes = Math.abs(blocks) * 2
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)
  const months = Math.floor(days / 30)

  if (months > 0) return `${months}mo ${days % 30}d`
  if (days > 0) return `${days}d ${hours % 24}h`
  if (hours > 0) return `${hours}h ${minutes % 60}m`
  return `${minutes}m`
}

/** Format a token amount with sensible display decimals (min 2, max capped at 6) */
export function formatAmount(amount: number, decimals: number): string {
  const divisor = Math.pow(10, decimals)
  return (amount / divisor).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: Math.min(decimals, 6),
  })
}

/** Format a number as a percentage string */
export function formatPercent(value: number): string {
  return value.toLocaleString(undefined, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 2,
  }) + '%'
}
