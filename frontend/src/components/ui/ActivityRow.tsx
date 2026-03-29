import './ActivityRow.css'
import type { ReactNode } from 'react'

interface ActivityRowProps {
  left: ReactNode
  center: ReactNode
  right: ReactNode
  className?: string
}

export function ActivityRow({ left, center, right, className = '' }: ActivityRowProps) {
  return (
    <div className={`ds-activity-row ${className}`}>
      <div className="ds-activity-left">{left}</div>
      <div className="ds-activity-center">{center}</div>
      <div className="ds-activity-right">{right}</div>
    </div>
  )
}
