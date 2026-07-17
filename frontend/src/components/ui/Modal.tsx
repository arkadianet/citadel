import './Modal.css'
import { useEffect } from 'react'
import type { ReactNode } from 'react'

type ModalSize = 'sm' | 'md' | 'lg'

interface ModalProps {
  open: boolean
  onClose: () => void
  title: ReactNode
  size?: ModalSize
  footer?: ReactNode
  children: ReactNode
}

export function Modal({ open, onClose, title, size = 'md', footer, children }: ModalProps) {
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  if (!open) return null

  return (
    <div className="ds-modal-scrim" onClick={onClose}>
      <div
        className={`ds-modal ds-modal--${size}`}
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="ds-modal__header">
          <h3 className="ds-modal__title">{title}</h3>
          <button className="ds-modal__close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="ds-modal__body">{children}</div>
        {footer && <div className="ds-modal__footer">{footer}</div>}
      </div>
    </div>
  )
}
