import './StatTile.css'

interface StatTileProps {
  label: string
  value: string
  change?: string
  changeDirection?: 'up' | 'down' | 'stable'
  className?: string
}

export function StatTile({ label, value, change, changeDirection, className = '' }: StatTileProps) {
  return (
    <div className={`ds-stat-tile ${className}`}>
      <div className="ds-stat-label">{label}</div>
      <div className="ds-stat-value">{value}</div>
      {change && (
        <div className={`ds-stat-change ${changeDirection === 'down' ? 'danger' : changeDirection === 'stable' ? 'neutral' : 'success'}`}>
          {changeDirection === 'up' && '▲ '}
          {changeDirection === 'down' && '▼ '}
          {changeDirection === 'stable' && '◆ '}
          {change}
        </div>
      )}
    </div>
  )
}
