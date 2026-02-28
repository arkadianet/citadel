import './Sidebar.css'

type View = 'home' | 'sigmausd' | 'dexy' | 'lending' | 'dex' | 'hodlcoin' | 'bridge' | 'bonds' | 'timelocks' | 'router' | 'explorer' | 'burn' | 'utxo-management'

interface SidebarProps {
  view: View
  onNavigate: (view: View) => void
  isConnected: boolean
  capabilityTier?: string
  collapsed: boolean
  onToggleCollapse: () => void
}

const protocols: Array<{
  id: View
  name: string
  description: string
  icon: string
  comingSoon?: boolean
}> = [
  { id: 'sigmausd', name: 'SigmaUSD', description: 'AgeUSD Stablecoin', icon: 'sigmausd' },
  { id: 'dexy', name: 'Dexy', description: 'Oracle Pegged', icon: 'dexy' },
  { id: 'lending', name: 'Lending', description: 'Duckpools', icon: 'lending' },
  { id: 'dex', name: 'DEX', description: 'AMM Swaps', icon: 'dex' },
  { id: 'hodlcoin', name: 'HodlCoin', description: 'Hold & Earn', icon: 'hodlcoin' },
  { id: 'bridge', name: 'Rosen', description: 'Bridge', icon: 'rosen' },
  { id: 'bonds', name: 'Bonds', description: 'SigmaFi P2P', icon: 'bonds' },
  { id: 'timelocks', name: 'Timelocks', description: 'MewLock', icon: 'timelock' },
]

function ProtocolIcon({ icon }: { icon: string; className: string }) {
  const svgProps = { viewBox: '0 0 24 24', width: '100%', height: '100%', fill: 'none', stroke: 'currentColor', strokeWidth: 2 }
  let content: React.ReactNode = null

  switch (icon) {
    case 'sigmausd':
      // Dollar sign — stablecoin
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 2v20" />
          <path d="M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6" />
        </svg>
      )
      break
    case 'dexy':
      // Target/bullseye — oracle-pegged
      content = (
        <svg {...svgProps}>
          <circle cx="12" cy="12" r="10" />
          <circle cx="12" cy="12" r="6" />
          <circle cx="12" cy="12" r="2" />
        </svg>
      )
      break
    case 'lending':
      // Bank building — lending/finance
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M3 21h18" />
          <path d="M3 10h18" />
          <path d="M12 3l9 7H3z" />
          <path d="M7 10v11" />
          <path d="M12 10v11" />
          <path d="M17 10v11" />
        </svg>
      )
      break
    case 'dex':
      // Opposing arrows — swap/exchange
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M5 8h14" />
          <path d="M15 4l4 4-4 4" />
          <path d="M19 16H5" />
          <path d="M9 12l-4 4 4 4" />
        </svg>
      )
      break
    case 'hodlcoin':
      // Stacked layers — hold & accumulate
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 2L2 7l10 5 10-5-10-5z" />
          <path d="M2 17l10 5 10-5" />
          <path d="M2 12l10 5 10-5" />
        </svg>
      )
      break
    case 'bonds':
      // Handshake — P2P bonds/lending
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M20.42 4.58a5.4 5.4 0 00-7.65 0l-.77.78-.77-.78a5.4 5.4 0 00-7.65 0C1.46 6.7 1.33 10.28 4 13l8 8 8-8c2.67-2.72 2.54-6.3.42-8.42z" />
          <path d="M12 5.36V21" />
        </svg>
      )
      break
    case 'rosen':
      // Bridge arch — cross-chain bridge
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <path d="M2 18h20" />
          <path d="M4 18v-5a8 8 0 0116 0v5" />
          <path d="M9 18v-3" />
          <path d="M15 18v-3" />
        </svg>
      )
      break
    case 'timelock':
      // Clock with lock — time-locked tokens
      content = (
        <svg {...svgProps} strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" />
          <path d="M12 6v6l4 2" />
          <path d="M17 17l2 2" />
        </svg>
      )
      break
  }

  if (!content) return null
  return <div className="sidebar-dashboard-icon">{content}</div>
}

export function Sidebar({ view, onNavigate, isConnected, capabilityTier, collapsed, onToggleCollapse }: SidebarProps) {
  const canUseProtocols = isConnected && capabilityTier !== 'Basic'

  const handleDashboardClick = () => {
    if (view === 'home') {
      onToggleCollapse()
    } else {
      onNavigate('home')
    }
  }

  return (
    <nav className={`sidebar ${collapsed ? 'collapsed' : 'expanded'}`}>
      <div className="sidebar-items">
        {/* Dashboard link */}
        <button
          className={`sidebar-item ${view === 'home' ? 'active' : ''}`}
          onClick={handleDashboardClick}
        >
          <div className="sidebar-dashboard-icon">
            <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="3" width="7" height="7" rx="1" />
              <rect x="14" y="3" width="7" height="7" rx="1" />
              <rect x="3" y="14" width="7" height="7" rx="1" />
              <rect x="14" y="14" width="7" height="7" rx="1" />
            </svg>
          </div>
          <div className="sidebar-item-text">
            <span className="sidebar-item-name">Dashboard</span>
          </div>
          <span className="sidebar-tooltip">Dashboard</span>
        </button>

        <div className="sidebar-separator" />
        <div className="sidebar-section-label">Protocols</div>

        {/* Protocol items */}
        {protocols.map(p => {
          const enabled = !p.comingSoon && canUseProtocols
          return (
            <button
              key={p.icon}
              className={`sidebar-item ${!p.comingSoon && view === p.id ? 'active' : ''} ${!enabled ? 'disabled' : ''}`}
              onClick={enabled ? () => onNavigate(p.id) : undefined}
              disabled={!enabled}
            >
              <ProtocolIcon icon={p.icon} className="" />
              <div className="sidebar-item-text">
                <span className="sidebar-item-name">{p.name}</span>
                <span className="sidebar-item-desc">{p.description}</span>
              </div>
              {p.comingSoon && <span className="sidebar-soon-badge">Soon</span>}
              <span className="sidebar-tooltip">{p.name}{p.comingSoon ? ' (Soon)' : ''}</span>
            </button>
          )
        })}

        <div className="sidebar-separator" />
        <div className="sidebar-section-label">Tools</div>

        {/* SigUSD Router */}
        <button
          className={`sidebar-item ${view === 'router' ? 'active' : ''} ${!canUseProtocols ? 'disabled' : ''}`}
          onClick={canUseProtocols ? () => onNavigate('router') : undefined}
          disabled={!canUseProtocols}
        >
          <div className="sidebar-dashboard-icon">
            <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M3 12h6m6 0h6" />
              <path d="M12 3v6m0 6v6" />
              <path d="M5.6 5.6l4.2 4.2m4.4 4.4l4.2 4.2" />
              <path d="M18.4 5.6l-4.2 4.2m-4.4 4.4l-4.2 4.2" />
            </svg>
          </div>
          <div className="sidebar-item-text">
            <span className="sidebar-item-name">Router</span>
            <span className="sidebar-item-desc">SigUSD Routes</span>
          </div>
          <span className="sidebar-tooltip">SigUSD Router</span>
        </button>

        {/* Explorer */}
        <button
          className={`sidebar-item ${view === 'explorer' ? 'active' : ''} ${!isConnected ? 'disabled' : ''}`}
          onClick={isConnected ? () => onNavigate('explorer') : undefined}
          disabled={!isConnected}
        >
          <div className="sidebar-dashboard-icon">
            <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
          </div>
          <div className="sidebar-item-text">
            <span className="sidebar-item-name">Explorer</span>
            <span className="sidebar-item-desc">Blockchain</span>
          </div>
          <span className="sidebar-tooltip">Explorer</span>
        </button>

        {/* Token Burn */}
        <button
          className={`sidebar-item ${view === 'burn' ? 'active' : ''} ${!isConnected ? 'disabled' : ''}`}
          onClick={isConnected ? () => onNavigate('burn') : undefined}
          disabled={!isConnected}
        >
          <div className="sidebar-dashboard-icon">
            <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 22c-4.97 0-9-3.58-9-8 0-3.06 2.13-6.27 4-8 .67 2 2.37 3.41 4 4C11.38 7.56 10.74 3 14 1c.67 2.67 3 5.33 4 7 1 1.67 1 3.33 1 5 0 4.42-3.13 9-7 9z" />
            </svg>
          </div>
          <div className="sidebar-item-text">
            <span className="sidebar-item-name">Burn</span>
            <span className="sidebar-item-desc">Destroy Tokens</span>
          </div>
          <span className="sidebar-tooltip">Token Burn</span>
        </button>

        {/* UTXO Management */}
        <button
          className={`sidebar-item ${view === 'utxo-management' ? 'active' : ''} ${!isConnected ? 'disabled' : ''}`}
          onClick={isConnected ? () => onNavigate('utxo-management') : undefined}
          disabled={!isConnected}
        >
          <div className="sidebar-dashboard-icon">
            <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="3" width="7" height="7" rx="1" />
              <rect x="14" y="3" width="7" height="7" rx="1" />
              <rect x="3" y="14" width="7" height="7" rx="1" />
              <rect x="14" y="14" width="7" height="7" rx="1" />
            </svg>
          </div>
          <div className="sidebar-item-text">
            <span className="sidebar-item-name">UTXOs</span>
            <span className="sidebar-item-desc">Manage Boxes</span>
          </div>
          <span className="sidebar-tooltip">UTXO Management</span>
        </button>
      </div>
    </nav>
  )
}
