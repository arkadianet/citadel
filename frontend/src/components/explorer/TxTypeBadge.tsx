/**
 * TxTypeBadge — Colored pill badge showing transaction type.
 */

import { classifyTransaction } from '../../api/txClassifier'
import type { Transaction } from '../../api/explorer'

interface Props {
  tx: Transaction
}

export function TxTypeBadge({ tx }: Props) {
  try {
    const classification = classifyTransaction(tx)
    return (
      <span className={`tx-type-badge ${classification.cssClass}`}>
        {classification.label}
      </span>
    )
  } catch {
    return <span className="tx-type-badge tx-type-transfer">Transfer</span>
  }
}
