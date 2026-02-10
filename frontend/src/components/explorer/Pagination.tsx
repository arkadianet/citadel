/**
 * Pagination — Dynamic page navigation with ellipsis.
 *
 * Shows: [Prev] [1] ... [4] [5] [*6*] [7] [8] ... [42] [Next]
 */

interface Props {
  currentPage: number
  totalPages: number
  onPageChange: (page: number) => void
}

function getPageNumbers(current: number, total: number): (number | '...')[] {
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i)
  }

  const pages: (number | '...')[] = []

  // Always show first page
  pages.push(0)

  if (current > 3) {
    pages.push('...')
  }

  // Pages around current
  const start = Math.max(1, current - 2)
  const end = Math.min(total - 2, current + 2)
  for (let i = start; i <= end; i++) {
    pages.push(i)
  }

  if (current < total - 4) {
    pages.push('...')
  }

  // Always show last page
  pages.push(total - 1)

  return pages
}

export function Pagination({ currentPage, totalPages, onPageChange }: Props) {
  if (totalPages <= 1) return null

  const pages = getPageNumbers(currentPage, totalPages)

  return (
    <div className="explorer-pagination">
      <button
        className="btn btn-secondary btn-sm"
        disabled={currentPage === 0}
        onClick={() => onPageChange(currentPage - 1)}
      >
        Prev
      </button>

      <div className="pagination-pages">
        {pages.map((p, i) =>
          p === '...' ? (
            <span key={`ellipsis-${i}`} className="pagination-ellipsis">...</span>
          ) : (
            <button
              key={p}
              className={`pagination-page-btn ${p === currentPage ? 'active' : ''}`}
              onClick={() => onPageChange(p)}
            >
              {p + 1}
            </button>
          )
        )}
      </div>

      <button
        className="btn btn-secondary btn-sm"
        disabled={currentPage >= totalPages - 1}
        onClick={() => onPageChange(currentPage + 1)}
      >
        Next
      </button>
    </div>
  )
}
