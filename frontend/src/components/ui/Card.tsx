import './Card.css'
import type { ReactNode, CSSProperties } from 'react'

interface CardProps {
  children: ReactNode
  surface?: 'display' | 'action'
  className?: string
  style?: CSSProperties
  onClick?: () => void
}

interface CardSectionProps {
  children: ReactNode
  className?: string
}

export function Card({ children, surface = 'display', className = '', style, onClick }: CardProps) {
  return (
    <div
      className={`ds-card ds-card--${surface} ${className}`}
      style={style}
      onClick={onClick}
      role={onClick ? 'button' : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={onClick ? (e) => { if (e.key === 'Enter') onClick() } : undefined}
    >
      {children}
    </div>
  )
}

export function CardHeader({ children, className = '' }: CardSectionProps) {
  return <div className={`ds-card-header ${className}`}>{children}</div>
}

export function CardBody({ children, className = '' }: CardSectionProps) {
  return <div className={`ds-card-body ${className}`}>{children}</div>
}

export function CardFooter({ children, className = '' }: CardSectionProps) {
  return <div className={`ds-card-footer ${className}`}>{children}</div>
}
