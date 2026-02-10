import { useState } from 'react'
import { search } from '../../api/explorer'
import type { ExplorerRoute } from '../ExplorerTab'

interface ExplorerSearchProps {
  onNavigate: (route: ExplorerRoute) => void
}

export function ExplorerSearch({ onNavigate }: ExplorerSearchProps) {
  const [query, setQuery] = useState('')
  const [searching, setSearching] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSearch = async () => {
    const q = query.trim()
    if (!q) return

    setSearching(true)
    setError(null)
    try {
      const result = await search(q)
      switch (result.type) {
        case 'address':
          onNavigate({ page: 'address', id: result.id })
          break
        case 'transaction':
          onNavigate({ page: 'transaction', id: result.id })
          break
        case 'token':
          onNavigate({ page: 'token', id: result.id })
          break
        case 'block':
          onNavigate({ page: 'block', id: result.id })
          break
      }
      setQuery('')
    } catch {
      setError('Not found')
      setTimeout(() => setError(null), 2000)
    } finally {
      setSearching(false)
    }
  }

  return (
    <div className="explorer-search">
      <input
        type="text"
        className="explorer-search-input"
        placeholder="Search address, tx, token, or block..."
        value={query}
        onChange={e => { setQuery(e.target.value); setError(null) }}
        onKeyDown={e => e.key === 'Enter' && handleSearch()}
        disabled={searching}
      />
      <button
        className="explorer-search-btn"
        onClick={handleSearch}
        disabled={searching || !query.trim()}
      >
        {searching ? (
          <span className="spinner-tiny" />
        ) : (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
        )}
      </button>
      {error && <span className="explorer-search-error">{error}</span>}
    </div>
  )
}
