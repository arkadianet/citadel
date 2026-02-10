import { useState } from 'react'
import { useExplorerNav } from '../contexts/ExplorerNavContext'

interface TxSuccessProps {
  txId: string
  explorerUrl: string
}

export function TxSuccess({ txId }: TxSuccessProps) {
  const [copied, setCopied] = useState(false)
  const { navigateToExplorer } = useExplorerNav()

  const handleCopy = async () => {
    await navigator.clipboard.writeText(txId)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="tx-success-box">
      <div className="tx-success-id">{txId}</div>
      <div className="tx-success-actions">
        <button className="tx-success-copy" onClick={handleCopy}>
          {copied ? 'Copied!' : 'Copy TX ID'}
        </button>
        <button
          className="tx-success-link"
          onClick={() => navigateToExplorer({ page: 'transaction', id: txId })}
        >
          View in Explorer
        </button>
      </div>
    </div>
  )
}
