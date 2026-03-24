import { useState } from 'react'
import { RouteCard } from './RouteCard'
import { formatTokenAmount } from '../utils/format'
import type { RouteQuote, SplitRouteDetail } from '../api/router'

// =============================================================================
// RouteList Props
// =============================================================================

export interface RouteListProps {
  routes: RouteQuote[]
  split: SplitRouteDetail | null
  selectedIndex: number
  onSelectRoute: (index: number) => void
  useSplit: boolean
  onToggleSplit: (use: boolean) => void
}

// =============================================================================
// RouteList
// =============================================================================

export function RouteList({
  routes,
  split,
  selectedIndex,
  onSelectRoute,
  useSplit,
  onToggleSplit,
}: RouteListProps) {
  const [altExpanded, setAltExpanded] = useState(false)

  const bestRoute = routes[0]
  const altRoutes = routes.slice(1)

  if (!bestRoute) return null

  return (
    <div className="smart-route-list">
      {/* Best route — full card */}
      <RouteCard
        routeQuote={bestRoute}
        isBest={true}
        isSelected={selectedIndex === 0 && !useSplit}
        onSelect={() => { onToggleSplit(false); onSelectRoute(0) }}
      />

      {/* Alternatives */}
      {altRoutes.length > 0 && (
        <div className="smart-route-alternatives">
          <button
            className="smart-route-alternatives-toggle"
            onClick={() => setAltExpanded(v => !v)}
            type="button"
          >
            {altExpanded
              ? `▾ Hide ${altRoutes.length} other ${altRoutes.length === 1 ? 'route' : 'routes'}`
              : `▸ ${altRoutes.length} other ${altRoutes.length === 1 ? 'route' : 'routes'} available`}
          </button>
          {altExpanded && altRoutes.map((rq, i) => (
            <RouteCard
              key={i}
              routeQuote={rq}
              isBest={false}
              isSelected={selectedIndex === i + 1 && !useSplit}
              onSelect={() => { onToggleSplit(false); onSelectRoute(i + 1) }}
              compact={true}
            />
          ))}
        </div>
      )}

      {/* Split suggestion */}
      {split !== null && (
        <div className={`smart-split-suggestion${useSplit ? ' active' : ''}`}>
          <div className="smart-split-header">
            <div className="smart-split-label">
              Split across {split.allocations.length} routes for{' '}
              <strong>+{split.improvement_pct.toFixed(2)}% better output</strong>
            </div>
            <button
              className={`smart-split-toggle${useSplit ? ' active' : ''}`}
              onClick={() => onToggleSplit(!useSplit)}
              type="button"
            >
              {useSplit ? 'Using split' : 'Use split'}
            </button>
          </div>

          {useSplit && (
            <div className="smart-split-allocations">
              {split.allocations.map((alloc, i) => {
                const lastHop = alloc.route.hops[alloc.route.hops.length - 1]
                const pathTokens = [
                  alloc.route.hops[0]?.token_in_name ?? '?',
                  ...alloc.route.hops.map(h => h.token_out_name ?? '?'),
                ]
                return (
                  <div key={i} className="smart-split-alloc">
                    <span className="smart-split-alloc-pct">
                      {(alloc.fraction * 100).toFixed(0)}%
                    </span>
                    <span className="smart-split-alloc-path">
                      {pathTokens.join(' → ')}
                    </span>
                    <span className="smart-split-alloc-output">
                      {formatTokenAmount(alloc.output_amount, lastHop?.token_out_decimals ?? 0)}{' '}
                      {lastHop?.token_out_name ?? ''}
                    </span>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
