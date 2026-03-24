import type { AmmPool } from '../api/amm'

// =============================================================================
// Token Icons
// =============================================================================

/** Map known token names (lowercase) to icon paths */
export const TOKEN_ICON_MAP: Record<string, string> = {
  // Core tokens
  erg: '/icons/ergo.svg',
  sigusd: '/icons/sigmausd.svg',
  sigrsv: '/icons/sigrsv.svg',
  rsn: '/icons/rosen.svg',
  rsada: '/icons/rsada.svg',
  spf: '/icons/spf.svg',
  rsbtc: '/icons/rsbtc.svg',
  quacks: '/icons/quacks.svg',
  // Bridge tokens
  rseth: '/icons/rseth.svg',
  rsbnb: '/icons/rsbnb.svg',
  rsdoge: '/icons/rsdoge.png',
  rsdis: '/icons/rsdis.png',
  // DeFi / ecosystem tokens
  ergopad: '/icons/ergopad.svg',
  neta: '/icons/neta.svg',
  paideia: '/icons/paideia.svg',
  exle: '/icons/exle.svg',
  epos: '/icons/epos.svg',
  flux: '/icons/flux.svg',
  terahertz: '/icons/terahertz.svg',
  gort: '/icons/gort.png',
  gluon: '/icons/gluon.png',
  // Dexy tokens
  dexygold: '/icons/dexygold.svg',
  use: '/icons/use.svg',
  // Meme / community tokens
  erdoge: '/icons/erdoge.svg',
  ermoon: '/icons/ermoon.svg',
  kushti: '/icons/kushti.svg',
  comet: '/icons/comet.png',
  aht: '/icons/aht.svg',
  burn: '/icons/burn.svg',
  getblok: '/icons/getblock.svg',
  hodlerg: '/icons/hodlerg3.svg',
  hodlerg3: '/icons/hodlerg3.svg',
  ergold: '/icons/ergold.svg',
  egio: '/icons/egio.svg',
  woodennickels: '/icons/woodennickels.svg',
  migoreng: '/icons/migoreng.svg',
  greasycex: '/icons/greasycex.svg',
  bober: '/icons/bober.png',
  bulls: '/icons/bulls.png',
  buns: '/icons/buns.png',
  cypx: '/icons/cypx.png',
  ergonaut: '/icons/ergonaut.png',
  ergone: '/icons/ergone.png',
  gauc: '/icons/gauc.png',
  gau: '/icons/gau.png',
  gif: '/icons/gif.png',
  ketchup: '/icons/ketchup.png',
  love: '/icons/love.png',
  lunadog: '/icons/lunadog.png',
  lykos: '/icons/lykos.png',
  mew: '/icons/mew.png',
  mustard: '/icons/mustard.png',
  oink: '/icons/oink.png',
  obsidian: '/icons/obsidian.png',
  pandav: '/icons/pandav.png',
  peperg: '/icons/peperg.png',
  php: '/icons/php.png',
  proxie: '/icons/proxie.png',
  walrus: '/icons/walrus.png',
  auctioncoin: '/icons/auctioncoin.png',
}

/** Deterministic color from token name for fallback circles */
export function tokenColor(name: string): string {
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash)
  const hue = ((hash % 360) + 360) % 360
  return `hsl(${hue}, 55%, 55%)`
}

export function TokenIcon({ name, size = 18 }: { name: string; size?: number }) {
  const icon = TOKEN_ICON_MAP[name.toLowerCase()]
  if (icon) {
    return <img src={icon} alt={name} className="token-icon" style={{ width: size, height: size }} />
  }
  return (
    <span
      className="token-icon-fallback"
      style={{ width: size, height: size, background: tokenColor(name), fontSize: size * 0.55 }}
    >
      {name.charAt(0).toUpperCase()}
    </span>
  )
}

export function PoolPairIcons({ pool }: { pool: AmmPool }) {
  const nameX = pool.pool_type === 'N2T' ? 'ERG' : (pool.token_x?.name || 'X')
  const nameY = pool.token_y.name || 'Y'
  return (
    <span className="pool-pair-icons">
      <TokenIcon name={nameX} size={18} />
      <TokenIcon name={nameY} size={18} />
    </span>
  )
}
