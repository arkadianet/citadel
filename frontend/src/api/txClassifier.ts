/**
 * txClassifier — Heuristic transaction type classification.
 *
 * Classifies transactions by inspecting inputs/outputs for known patterns:
 *  - Block Reward: first input boxId is all zeros
 *  - DEX Swap: outputs contain known Spectrum pool NFT token IDs
 *  - SigmaUSD: inputs/outputs contain SigmaUSD bank box token ID
 *  - Transfer: default fallback
 */

import type { Transaction } from './explorer'

export interface TxClassification {
  type: 'reward' | 'dex' | 'sigmausd' | 'transfer'
  label: string
  cssClass: string
}

// SigmaUSD bank box NFT
const SIGMAUSD_BANK_NFT = '7d672d1def471720ca5782fd6473e47e796d9ac0c138d9911346f118b2f6d9d9'

// Known Spectrum DEX pool NFT token IDs
const SPECTRUM_POOL_NFTS = new Set([
  // ERG/SigUSD
  '1d5afc3361cada1feaaaf0e87d192326e8e023d8d2d1b78b3cc42fce7b0e5a42',
  // ERG/SigRSV
  'c06ee7b1f09816e981c1b3e9c5bae33d1cdee0e7d845bea5c44dbeaac5b36d62',
  // ERG/SPF
  'b2f26f25a6cdc9f0acc0de33f3e7e55ffe3dab2cf6e1433f70dce7917e0e6e80',
  // ERG/NETA
  '7a540fc23fc8e2b14ff42b4fea498ffc37995dcccc1db78d62d54a2f49a3b5cb',
  // ERG/Paideia
  '10b0a7beac0e7e4a71e7b4e5f7b24862a1c38e8b7f0a43a2a3beab35c83ea5d7',
])

const ZERO_BOX_PREFIX = '0000000000000000'

export function classifyTransaction(tx: Transaction): TxClassification {
  // Block Reward: first input's boxId starts with zeros (coinbase)
  if (tx.inputs.length > 0 && tx.inputs[0].boxId.startsWith(ZERO_BOX_PREFIX)) {
    return { type: 'reward', label: 'Block Reward', cssClass: 'tx-type-reward' }
  }

  // Collect all token IDs from inputs and outputs
  const allTokenIds = new Set<string>()
  for (const input of tx.inputs) {
    const assets = (input as Record<string, unknown>).assets as { tokenId: string }[] | undefined
    if (assets) {
      for (const a of assets) allTokenIds.add(a.tokenId)
    }
  }
  for (const output of tx.outputs) {
    if (output.assets) {
      for (const a of output.assets) allTokenIds.add(a.tokenId)
    }
  }

  // SigmaUSD: contains bank box NFT
  if (allTokenIds.has(SIGMAUSD_BANK_NFT)) {
    return { type: 'sigmausd', label: 'SigmaUSD', cssClass: 'tx-type-sigmausd' }
  }

  // DEX Swap: contains Spectrum pool NFT
  for (const nft of SPECTRUM_POOL_NFTS) {
    if (allTokenIds.has(nft)) {
      return { type: 'dex', label: 'DEX Swap', cssClass: 'tx-type-dex' }
    }
  }

  return { type: 'transfer', label: 'Transfer', cssClass: 'tx-type-transfer' }
}
