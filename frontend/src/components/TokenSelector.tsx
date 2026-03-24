import { useState, useRef, useEffect, useMemo } from 'react'
import { TokenIcon } from './tokenIcons'
import { formatTokenAmount } from '../utils/format'

export interface TokenEntry {
  token_id: string
  name: string | null
  decimals: number
  balance?: number // raw units, present if user holds it
}

interface TokenSelectorProps {
  tokens: TokenEntry[]
  selected: TokenEntry | null
  onSelect: (token: TokenEntry) => void
  placeholder?: string
  disabled?: boolean
}

export function TokenSelector({
  tokens,
  selected,
  onSelect,
  placeholder = 'Select token',
  disabled = false,
}: TokenSelectorProps) {
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState('')
  const wrapperRef = useRef<HTMLDivElement>(null)

  // Close on outside click
  useEffect(() => {
    if (!open) return
    function handleMouseDown(e: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handleMouseDown)
    return () => document.removeEventListener('mousedown', handleMouseDown)
  }, [open])

  // Reset search when dropdown closes
  useEffect(() => {
    if (!open) setSearch('')
  }, [open])

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase()
    if (!query) return tokens
    return tokens.filter((t) => {
      const name = (t.name ?? '').toLowerCase()
      return name.includes(query) || t.token_id.toLowerCase().includes(query)
    })
  }, [tokens, search])

  function handleToggle() {
    if (disabled) return
    setOpen((v) => !v)
  }

  function handleSelect(token: TokenEntry) {
    onSelect(token)
    setOpen(false)
  }

  const displayName = selected
    ? (selected.name ?? selected.token_id.slice(0, 8))
    : null

  return (
    <div className="token-selector" ref={wrapperRef} style={disabled ? { opacity: 0.5 } : undefined}>
      <button
        type="button"
        className="token-selector-trigger"
        onClick={handleToggle}
        disabled={disabled}
      >
        {displayName !== null ? (
          <>
            <TokenIcon name={selected!.name ?? selected!.token_id.slice(0, 8)} />
            <span className="token-selector-name">{displayName}</span>
          </>
        ) : (
          <span className="token-selector-placeholder">{placeholder}</span>
        )}
        <span className="token-selector-arrow">▾</span>
      </button>

      {open && (
        <div className="token-selector-dropdown">
          <input
            className="token-selector-search"
            type="text"
            placeholder="Search by name or ID…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            autoFocus
          />
          <div className="token-selector-list">
            {filtered.length === 0 ? (
              <div className="token-selector-empty">No tokens found</div>
            ) : (
              filtered.map((token) => {
                const itemName = token.name ?? token.token_id.slice(0, 8)
                const isActive = selected?.token_id === token.token_id
                return (
                  <button
                    key={token.token_id}
                    type="button"
                    className={`token-selector-item${isActive ? ' active' : ''}`}
                    onClick={() => handleSelect(token)}
                  >
                    <TokenIcon name={itemName} />
                    <span className="token-selector-item-name">{itemName}</span>
                    {token.balance !== undefined && (
                      <span className="token-selector-item-balance">
                        {formatTokenAmount(token.balance, token.decimals)}
                      </span>
                    )}
                  </button>
                )
              })
            )}
          </div>
        </div>
      )}
    </div>
  )
}
