import './PageHeader.css'
import type { ReactNode } from 'react'

interface InfoItem {
  label: string
  value: string
}

interface PageHeaderProps {
  icon: ReactNode
  title: string
  subtitle?: string
  info?: InfoItem[]
  actions?: ReactNode
  className?: string
}

export function PageHeader({ icon, title, subtitle, info, actions, className = '' }: PageHeaderProps) {
  return (
    <div className={`ds-page-header ${className}`}>
      <div className="ds-page-header-row">
        <div className="ds-page-header-left">
          <div className="ds-page-header-icon">{icon}</div>
          <div>
            <h2 className="ds-page-title">{title}</h2>
            {subtitle && <p className="ds-page-subtitle">{subtitle}</p>}
          </div>
        </div>
        {actions && <div className="ds-page-header-actions">{actions}</div>}
      </div>

      {info && info.length > 0 && (
        <div className="ds-page-info-bar">
          {info.map((item, i) => (
            <span key={item.label} className="ds-page-info-item">
              {i > 0 && <span className="ds-page-info-divider" />}
              <span className="ds-page-info-label">{item.label}</span>
              <span className="ds-page-info-value">{item.value}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  )
}
