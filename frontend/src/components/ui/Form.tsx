import './Form.css'
import type { InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react'

type ControlSize = 'sm' | 'md'

interface FormFieldProps {
  label: ReactNode
  hint?: ReactNode
  error?: ReactNode
  children: ReactNode
}

export function FormField({ label, hint, error, children }: FormFieldProps) {
  return (
    <label className="ds-field">
      <span className="ds-field__label">{label}</span>
      {children}
      {error ? (
        <span className="ds-field__error">{error}</span>
      ) : (
        hint && <span className="ds-field__hint">{hint}</span>
      )}
    </label>
  )
}

interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'size'> {
  size?: ControlSize
  invalid?: boolean
}

export function Input({ size = 'md', invalid = false, className = '', ...rest }: InputProps) {
  return (
    <input
      className={`ds-input ds-input--${size} ${invalid ? 'ds-input--invalid' : ''} ${className}`}
      {...rest}
    />
  )
}

interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, 'size'> {
  size?: ControlSize
}

export function Select({ size = 'md', className = '', children, ...rest }: SelectProps) {
  return (
    <select className={`ds-input ds-select ds-input--${size} ${className}`} {...rest}>
      {children}
    </select>
  )
}
