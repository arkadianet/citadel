import './FilterBar.css'
import type { ReactNode } from 'react'

interface FilterOption {
  id: string
  label: string
}

interface FilterBarProps {
  options: FilterOption[]
  activeId: string
  onChange: (id: string) => void
  actions?: ReactNode
  className?: string
}

export function FilterBar({ options, activeId, onChange, actions, className = '' }: FilterBarProps) {
  return (
    <div className={`ds-filter-bar ${className}`}>
      <div className="ds-filter-options">
        {options.map(opt => (
          <button
            key={opt.id}
            className={`ds-filter-btn ${activeId === opt.id ? 'active' : ''}`}
            onClick={() => onChange(opt.id)}
          >
            {opt.label}
          </button>
        ))}
      </div>
      {actions && <div className="ds-filter-actions">{actions}</div>}
    </div>
  )
}
