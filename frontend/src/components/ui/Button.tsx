import './Button.css'
import type { ButtonHTMLAttributes } from 'react'

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost'
type ButtonSize = 'sm' | 'md'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  loading?: boolean
}

export function Button({
  variant = 'secondary',
  size = 'md',
  loading = false,
  disabled,
  className = '',
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      className={`ds-btn ds-btn--${variant} ds-btn--${size} ${className}`}
      disabled={disabled || loading}
      {...rest}
    >
      {loading && <span className="ds-btn__spinner" aria-hidden="true" />}
      {children}
    </button>
  )
}
