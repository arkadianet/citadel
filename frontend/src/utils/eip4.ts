/**
 * EIP-4 helpers — decode Sigma Coll[Byte] registers and resolve NFT artwork URLs.
 * Node token/byId often has name/decimals but not artwork; R9 on the issuance box
 * usually holds the media URL (Coll[Byte] UTF-8). Nested Coll[Coll[Byte]] is also common.
 */

function hexToBytes(hex: string): Uint8Array {
  const clean = hex.trim().replace(/^0x/i, '')
  if (clean.length % 2 !== 0) return new Uint8Array()
  const out = new Uint8Array(clean.length / 2)
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16)
  }
  return out
}

function readVlq(bytes: Uint8Array, offset: number): { value: number; next: number } | null {
  let value = 0
  let shift = 0
  let i = offset
  while (i < bytes.length) {
    const b = bytes[i++]
    value |= (b & 0x7f) << shift
    if ((b & 0x80) === 0) return { value, next: i }
    shift += 7
    if (shift > 28) return null
  }
  return null
}

/** Decode a Sigma constant hex that is Coll[Byte] (0x0e) into UTF-8 text when possible. */
export function decodeCollByteUtf8(sigmaHex: string | undefined | null): string | null {
  if (!sigmaHex) return null
  // Some APIs already return rendered strings
  if (!/^[0-9a-fA-Fx]+$/.test(sigmaHex.trim()) && sigmaHex.includes('http')) {
    return sigmaHex.trim()
  }
  const bytes = hexToBytes(sigmaHex)
  if (bytes.length < 2 || bytes[0] !== 0x0e) return null
  const len = readVlq(bytes, 1)
  if (!len) return null
  const data = bytes.slice(len.next, len.next + len.value)
  if (data.length !== len.value) return null
  try {
    const text = new TextDecoder('utf-8', { fatal: false }).decode(data).trim()
    return text || null
  } catch {
    return null
  }
}

/**
 * Extract artwork URL from issuance-box registers.
 * Prefer R9; fall back to scanning R7–R9 for http(s)/ipfs URLs.
 */
export function artworkUrlFromRegisters(
  registers: Record<string, string> | undefined | null,
): string | null {
  if (!registers) return null
  const keys = ['R9', 'R8', 'R7']
  for (const key of keys) {
    const raw = registers[key]
    if (!raw) continue
    const decoded = decodeCollByteUtf8(raw) ?? (raw.includes('://') ? raw : null)
    if (!decoded) continue
    // Nested: first line / JSON-ish — take first URL-looking token
    const match = decoded.match(/(ipfs:\/\/[^\s"'<>]+|https?:\/\/[^\s"'<>]+)/i)
    if (match) return match[1]
    if (decoded.startsWith('ipfs://') || /^https?:\/\//i.test(decoded)) return decoded
  }
  return null
}

/** Normalize IPFS / common gateways for <img src>. */
export function resolveMediaUrl(url: string | null | undefined): string | null {
  if (!url) return null
  const trimmed = url.trim()
  if (!trimmed) return null
  if (trimmed.startsWith('ipfs://')) {
    const path = trimmed.slice('ipfs://'.length).replace(/^ipfs\//, '')
    return `https://ipfs.io/ipfs/${path}`
  }
  if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) return trimmed
  return null
}

/** Heuristic: single-unit, zero-decimal tokens are typically NFT-like (not fungible). */
export function isNftLikeToken(t: {
  amount: number
  amount_str?: string
  decimals: number
  emissionAmount?: number | null
}): boolean {
  const raw =
    t.amount_str !== undefined ? BigInt(t.amount_str) : BigInt(Math.trunc(t.amount))
  if (raw !== 1n) return false
  if (t.decimals !== 0) return false
  if (t.emissionAmount != null && t.emissionAmount > 1) return false
  return true
}
