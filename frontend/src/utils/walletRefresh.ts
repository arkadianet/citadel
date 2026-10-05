export const WALLET_TX_SUBMITTED = 'citadel:wallet-tx-submitted'

/** Refresh mempool balances as soon as the node accepts a wallet transaction. */
export function notifyWalletTransactionSubmitted(): void {
  window.dispatchEvent(new Event(WALLET_TX_SUBMITTED))
}
