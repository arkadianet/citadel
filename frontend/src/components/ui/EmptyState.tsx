import './EmptyState.css'
import type { ReactNode } from 'react'

interface EmptyStateProps {
  icon?: ReactNode
  title: string
  description?: string
  action?: ReactNode
  className?: string
}

export function EmptyState({ icon, title, description, action, className = '' }: EmptyStateProps) {
  return (
    <div className={`ds-empty-state ${className}`}>
      {icon && <div className="ds-empty-icon">{icon}</div>}
      <h3 className="ds-empty-title">{title}</h3>
      {description && <p className="ds-empty-desc">{description}</p>}
      {action && <div className="ds-empty-action">{action}</div>}
    </div>
  )
}
