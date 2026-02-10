/**
 * ExplorerSkeleton — Shimmer loading placeholders for explorer views.
 *
 * Three variants:
 *  - table:  rows × columns of shimmer bars
 *  - card:   key-value shimmer rows (info card shape)
 *  - text:   single line shimmer
 */

interface TableSkeletonProps {
  variant: 'table'
  rows?: number
  columns?: number
}

interface CardSkeletonProps {
  variant: 'card'
  rows?: number
}

interface TextSkeletonProps {
  variant: 'text'
  width?: string
}

type SkeletonProps = TableSkeletonProps | CardSkeletonProps | TextSkeletonProps

export function ExplorerSkeleton(props: SkeletonProps) {
  switch (props.variant) {
    case 'table': {
      const rows = props.rows ?? 8
      const cols = props.columns ?? 5
      return (
        <div className="skeleton-table">
          <div className="skeleton-table-header">
            {Array.from({ length: cols }, (_, i) => (
              <div key={i} className="skeleton-bar" style={{ width: i === 0 ? '30%' : '15%' }} />
            ))}
          </div>
          {Array.from({ length: rows }, (_, r) => (
            <div key={r} className="skeleton-table-row">
              {Array.from({ length: cols }, (_, c) => (
                <div key={c} className="skeleton-bar" style={{ width: c === 0 ? '60%' : `${40 + (c * 7) % 30}%` }} />
              ))}
            </div>
          ))}
        </div>
      )
    }
    case 'card': {
      const rows = props.rows ?? 6
      return (
        <div className="skeleton-card">
          {Array.from({ length: rows }, (_, i) => (
            <div key={i} className="skeleton-card-row">
              <div className="skeleton-bar" style={{ width: '90px' }} />
              <div className="skeleton-bar" style={{ width: `${50 + (i * 13) % 40}%` }} />
            </div>
          ))}
        </div>
      )
    }
    case 'text':
      return <div className="skeleton-bar" style={{ width: props.width ?? '200px', height: '14px' }} />
  }
}
