import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import './AdvancedOptions.css'

interface AdvancedOptionsProps {
  recipientAddress: string
  onRecipientChange: (address: string) => void
  addressValid: boolean | null
}

export function AdvancedOptions({
  recipientAddress,
  onRecipientChange,
  addressValid,
}: AdvancedOptionsProps) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className="advanced-options">
      <button
        className="advanced-toggle"
        onClick={() => setExpanded(e => !e)}
        type="button"
      >
        <svg
          width="12" height="12" viewBox="0 0 24 24"
          fill="none" stroke="currentColor" strokeWidth="2"
          className={`advanced-chevron ${expanded ? 'open' : ''}`}
        >
          <polyline points="9 18 15 12 9 6" />
        </svg>
        Advanced
      </button>

      {expanded && (
        <div className="advanced-body">
          <div className="advanced-field">
            <label>Recipient address</label>
            <span className="advanced-field-hint">
              Leave empty to receive at your wallet address
            </span>
            <input
              type="text"
              value={recipientAddress}
              onChange={e => onRecipientChange(e.target.value)}
              placeholder="9..."
              className={`advanced-input ${
                recipientAddress && addressValid === false ? 'invalid' : ''
              } ${recipientAddress && addressValid === true ? 'valid' : ''}`}
            />
            {recipientAddress && addressValid === false && (
              <span className="advanced-error">Invalid Ergo address</span>
            )}
            {recipientAddress && addressValid === true && (
              <span className="advanced-valid">Valid address</span>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * Hook to manage recipient address state + validation.
 */
export function useRecipientAddress() {
  const [recipientAddress, setRecipientAddress] = useState('')
  const [addressValid, setAddressValid] = useState<boolean | null>(null)

  useEffect(() => {
    if (!recipientAddress.trim()) {
      setAddressValid(null)
      return
    }

    const timeout = setTimeout(async () => {
      try {
        await invoke<string>('validate_ergo_address', { address: recipientAddress.trim() })
        setAddressValid(true)
      } catch {
        setAddressValid(false)
      }
    }, 400)

    return () => clearTimeout(timeout)
  }, [recipientAddress])

  const recipientOrNull = recipientAddress.trim() && addressValid === true
    ? recipientAddress.trim()
    : null

  return { recipientAddress, setRecipientAddress, addressValid, recipientOrNull } as const
}
