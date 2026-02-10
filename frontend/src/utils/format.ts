/**
 * Format a raw token amount for display using its decimal places.
 */
export function formatTokenAmount(amount: number, decimals: number): string {
  if (decimals === 0) return amount.toLocaleString()
  const divisor = Math.pow(10, decimals)
  return (amount / divisor).toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  })
}

/**
 * Format nanoERG amount for display (9 decimals).
 */
export function formatErg(nanoErg: number): string {
  return formatTokenAmount(nanoErg, 9)
}
