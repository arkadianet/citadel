import { MIN_BOX_VALUE_NANO, TX_FEE_NANO, MAX_RESTRUCTURE_OUTPUTS } from '../../constants'
import { formatErg } from '../../utils/format'
import { displayRawAmount } from './tokenIsolation'
import type { IsolationAllocation, IsolationPlan, IsolationToken } from './tokenIsolation'

interface Props {
  tokens: IsolationToken[]
  allocations: IsolationAllocation[]
  plan: IsolationPlan
  inputCount: number
  totalNano: number
  canAddFunding: boolean
  getTokenName: (id: string, fallback: string | null) => string
  onChange: (allocations: IsolationAllocation[]) => void
  onAddFunding: () => void
  onPreview: () => void
}

export function UtxoTokenIsolationPanel({ tokens, allocations, plan, inputCount, totalNano,
  canAddFunding, getTokenName, onChange, onAddFunding, onPreview }: Props) {
  const defaultAllocation = (token: IsolationToken): IsolationAllocation => ({
    tokenId: token.tokenId, amount: displayRawAmount(token.rawAmount, token.decimals), erg: '0.001',
  })
  const update = (tokenId: string, field: 'amount' | 'erg', value: string) =>
    onChange(allocations.map(a => a.tokenId === tokenId ? { ...a, [field]: value } : a))

  return <>
    <section className="utxo-side-section">
      <h3 className="utxo-side-title">Separate tokens &amp; NFTs</h3>
      <p className="utxo-isolation-hint">Each checked token gets its own UTXO. Select NETA stake keys here to prepare them for recovery.</p>
      {tokens.length === 0 ? (
        <p className="utxo-split-empty-hint">Select a box containing tokens or NFTs on the board.</p>
      ) : <>
        <div className="utxo-isolation-actions">
          <button type="button" className="utxo-toolbar-btn"
            disabled={tokens.length > MAX_RESTRUCTURE_OUTPUTS}
            onClick={() => onChange(tokens.map(defaultAllocation))}>Select all tokens</button>
          <button type="button" className="utxo-toolbar-btn" onClick={() => onChange([])}>Clear tokens</button>
        </div>
        <div className="utxo-isolation-tokens">
          {tokens.map(token => {
            const allocation = allocations.find(a => a.tokenId === token.tokenId)
            const name = getTokenName(token.tokenId, token.name)
            return <div key={token.tokenId} className={`utxo-isolation-token${allocation ? ' selected' : ''}`}>
              <label className="utxo-isolation-select">
                <input type="checkbox" checked={!!allocation}
                  disabled={!allocation && allocations.length >= MAX_RESTRUCTURE_OUTPUTS}
                  onChange={e => onChange(e.target.checked
                    ? [...allocations, defaultAllocation(token)]
                    : allocations.filter(a => a.tokenId !== token.tokenId))} />
                <span><strong>{name}</strong><span className="utxo-isolation-available mono">
                  {displayRawAmount(token.rawAmount, token.decimals)} available
                </span></span>
              </label>
              <code className="utxo-isolation-id mono">{token.tokenId}</code>
              {allocation && <div className="utxo-isolation-fields">
                <label>Amount
                  <input type="text" inputMode="decimal" className="utxo-split-input" aria-label={`Amount for ${name}`}
                    value={allocation.amount} onChange={e => update(token.tokenId, 'amount', e.target.value)} />
                </label>
                <button type="button" className="utxo-toolbar-btn"
                  onClick={() => update(token.tokenId, 'amount', displayRawAmount(token.rawAmount, token.decimals))}>Max</button>
                <label>ERG in this box
                  <input type="text" inputMode="decimal" className="utxo-split-input" aria-label={`ERG for ${name}`}
                    value={allocation.erg} onChange={e => update(token.tokenId, 'erg', e.target.value)} />
                </label>
              </div>}
            </div>
          })}
        </div>
      </>}
    </section>
    <section className="utxo-side-section">
      <h3 className="utxo-side-title">Balance</h3>
      <div className="utxo-settings">
        <div className="utxo-setting-row"><span>Selected inputs</span><span>{inputCount} · {formatErg(totalNano)} ERG</span></div>
        <div className="utxo-setting-row"><span>Separate outputs</span><span>{allocations.length}</span></div>
        <div className="utxo-setting-row"><span>Output ERG</span><span>{formatErg(plan.allocatedNano)} ERG</span></div>
        <div className="utxo-setting-row"><span>Miner fee</span><span>{formatErg(TX_FEE_NANO)} ERG</span></div>
        <div className="utxo-setting-row"><span>Change ERG</span><span>{formatErg(Math.max(0, plan.changeNano))} ERG</span></div>
        <div className="utxo-setting-row"><span>Tokens in change</span><span>{plan.remainingTokens.length} types</span></div>
      </div>
      <p className="utxo-isolation-hint">Unselected tokens and remaining amounts return to your wallet as change. Each output needs at least {formatErg(MIN_BOX_VALUE_NANO)} ERG. A change box holding many token types may need more.</p>
      {plan.fundingNeededNano > 0 && <button type="button" className="utxo-toolbar-btn"
        disabled={!canAddFunding} onClick={onAddFunding}>Add ERG funding input</button>}
      {plan.fundingNeededNano > 0 && !canAddFunding && <p className="utxo-isolation-hint">Select another funded box on the board. Its unassigned assets will return as change.</p>}
    </section>
    {plan.error && <p className="utxo-erg-status utxo-erg-status--under-min" role="status">{plan.error}</p>}
    <button type="button" className="utxo-submit-btn" onClick={onPreview}
      disabled={inputCount === 0 || !!plan.error}>Review separate outputs</button>
  </>
}
