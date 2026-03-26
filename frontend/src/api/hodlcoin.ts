/**
 * HodlCoin Protocol API
 *
 * TypeScript types and invoke wrappers for Phoenix HodlCoin protocol Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'


// =============================================================================
// Type Definitions
// =============================================================================

export interface HodlBankState {
  bankBoxId: string
  singletonTokenId: string
  hodlTokenId: string
  hodlTokenName: string | null
  totalTokenSupply: number
  precisionFactor: number
  minBankValue: number
  devFeeNum: number
  bankFeeNum: number
  reserveNanoErg: number
  hodlTokensInBank: number
  circulatingSupply: number
  priceNanoPerHodl: number
  tvlNanoErg: number
  totalFeePct: number
  bankFeePct: number
  devFeePct: number
}

export interface HodlMintPreview {
  ergDeposited: number
  hodlTokensReceived: number
  pricePerToken: number
  minerFee: number
  totalErgCost: number
}

export interface HodlBurnPreview {
  hodlTokensSpent: number
  ergReceived: number
  bankFeeNano: number
  devFeeNano: number
  ergBeforeFees: number
  pricePerToken: number
  minerFee: number
}

// =============================================================================
// API Functions
// =============================================================================

export async function getHodlCoinBanks(): Promise<HodlBankState[]> {
  return await invoke<HodlBankState[]>('get_hodlcoin_banks')
}

export async function previewHodlCoinMint(
  singletonTokenId: string,
  ergAmount: number,
): Promise<HodlMintPreview> {
  return await invoke<HodlMintPreview>('preview_hodlcoin_mint', {
    singletonTokenId,
    ergAmount,
  })
}

export async function previewHodlCoinBurn(
  singletonTokenId: string,
  hodlAmount: number,
): Promise<HodlBurnPreview> {
  return await invoke<HodlBurnPreview>('preview_hodlcoin_burn', {
    singletonTokenId,
    hodlAmount,
  })
}

export async function buildHodlCoinMintTx(
  singletonTokenId: string,
  ergAmount: number,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('build_hodlcoin_mint_tx', {
    singletonTokenId,
    ergAmount,
    userUtxos,
    currentHeight,
  })
}

export async function buildHodlCoinBurnTx(
  singletonTokenId: string,
  hodlAmount: number,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('build_hodlcoin_burn_tx', {
    singletonTokenId,
    hodlAmount,
    userUtxos,
    currentHeight,
  })
}

// =============================================================================
// Helper Functions
// =============================================================================

export function formatHodlPrice(priceNano: number): string {
  return (priceNano / 1_000_000_000).toLocaleString(undefined, {
    minimumFractionDigits: 6,
    maximumFractionDigits: 9,
  })
}
