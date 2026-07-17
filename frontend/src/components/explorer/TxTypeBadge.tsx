/**
 * TxTypeBadge — Colored pill badge showing transaction type.
 *
 * Thin wrapper over the shared ui <Badge>.
 */

import { Badge } from '../ui'
import { classifyTransaction } from '../../api/txClassifier'
import type { Transaction } from '../../api/explorer'

interface Props {
  tx: Transaction
}

const TYPE_VARIANT: Record<string, 'success' | 'warning' | 'info' | 'neutral'> = {
  reward: 'info',
  dex: 'success',
  sigmausd: 'warning',
  transfer: 'neutral',
}

export function TxTypeBadge({ tx }: Props) {
  try {
    const classification = classifyTransaction(tx)
    return (
      <Badge variant={TYPE_VARIANT[classification.type] ?? 'neutral'}>
        {classification.label}
      </Badge>
    )
  } catch {
    return <Badge variant="neutral">Transfer</Badge>
  }
}
