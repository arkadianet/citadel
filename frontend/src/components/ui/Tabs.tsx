import './Tabs.css'

interface Tab {
  id: string
  label: string
  disabled?: boolean
}

interface TabsProps {
  tabs: Tab[]
  activeId: string
  onChange: (id: string) => void
  size?: 'default' | 'compact'
  className?: string
}

export function Tabs({ tabs, activeId, onChange, size = 'default', className = '' }: TabsProps) {
  return (
    <div className={`ds-tabs ds-tabs--${size} ${className}`} role="tablist">
      {tabs.map(tab => (
        <button
          key={tab.id}
          className={`ds-tab ${activeId === tab.id ? 'active' : ''}`}
          onClick={() => onChange(tab.id)}
          disabled={tab.disabled}
          role="tab"
          aria-selected={activeId === tab.id}
        >
          {tab.label}
        </button>
      ))}
    </div>
  )
}
