import './Loading.css'

export function Spinner({ size = 16 }: { size?: number }) {
  return (
    <span
      className="ds-spinner"
      style={{ width: size, height: size }}
      role="status"
      aria-label="Loading"
    />
  )
}

interface SkeletonProps {
  width?: string
  height?: string
  className?: string
}

export function Skeleton({ width = '100%', height = '16px', className = '' }: SkeletonProps) {
  return <span className={`ds-skeleton ${className}`} style={{ width, height }} aria-hidden="true" />
}
