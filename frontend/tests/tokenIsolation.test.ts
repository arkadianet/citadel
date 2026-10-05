import assert from 'node:assert/strict'
import { test } from 'node:test'
import { collectIsolationTokens, displayRawAmount, findIsolationFunding, parseDisplayUnits, planTokenIsolation } from '../src/components/utxo/tokenIsolation.ts'
import type { UtxoBox } from '../src/components/utxo/types.ts'

const keyA = 'a'.repeat(64)
const keyB = 'b'.repeat(64)
const fungible = 'c'.repeat(64)
const box = (boxId: string, value: string, assets: UtxoBox['assets'] = []): UtxoBox => ({
  boxId, value, assets, ergoTree: '0008cd', creationHeight: 1,
  transactionId: 'd'.repeat(64), index: 0, additionalRegisters: {}, extension: {},
})
const mixed = box('mixed', '1000000', [
  { tokenId: keyA, amount: '1' }, { tokenId: keyB, amount: '1' },
  { tokenId: fungible, amount: '9007199254740993' },
])
const tokens = collectIsolationTokens([mixed], [{ token_id: fungible, amount: 0, decimals: 6, name: 'Large balance' }])
const plan = (allocations: {tokenId: string, amount: string, erg: string}[], total = 10_000_000) =>
  planTokenIsolation(tokens, allocations, total, 1_100_000, 1_000_000, 120)

test('isolates two stake keys while preserving a mixed-box balance exactly in change', () => {
  const result = plan([
    { tokenId: keyB, amount: '1', erg: '0.001' },
    { tokenId: keyA, amount: '1', erg: '0.002' },
  ])
  assert.equal(result.error, null)
  assert.deepEqual(result.outputs, [
    { value: 1_000_000, tokens: [{ tokenId: keyB, amount: '1' }] },
    { value: 2_000_000, tokens: [{ tokenId: keyA, amount: '1' }] },
  ])
  assert.equal(result.remainingTokens[0].rawAmount, '9007199254740993')
  assert.equal(result.changeNano, 5_900_000)
  assert.equal(result.allocatedNano + result.changeNano + 1_100_000, 10_000_000)
})

test('partial allocations preserve integer precision above JavaScript safe integer', () => {
  const result = plan([{ tokenId: fungible, amount: '9007199254.740992', erg: '0.001' }])
  assert.equal(result.error, null)
  assert.equal(result.outputs[0].tokens[0].amount, '9007199254740992')
  assert.equal(result.remainingTokens.find(t => t.tokenId === fungible)?.rawAmount, '1')
  assert.equal(displayRawAmount('9007199254740993', 6), '9007199254.740993')
})

test('rejects invalid amounts, fractional NFTs, excessive precision and duplicate allocations', () => {
  for (const amount of ['0', '-1', '2', '0.5', '1e0', '1junk', '1.0000000001', 'NaN', 'Infinity']) {
    assert.notEqual(plan([{ tokenId: keyA, amount, erg: '0.001' }]).error, null, amount)
  }
  assert.equal(parseDisplayUnits('0.0000000001', 9), null)
  assert.equal(parseDisplayUnits('.001', 9), 1_000_000n)
  assert.notEqual(plan([{ tokenId: keyA, amount: '1', erg: '0.000999999' }]).error, null)
  assert.notEqual(plan([{ tokenId: keyA, amount: '1', erg: '0.0011x' }]).error, null)
  assert.notEqual(plan([
    { tokenId: keyA, amount: '1', erg: '0.001' },
    { tokenId: keyA, amount: '1', erg: '0.001' },
  ]).error, null)
})

test('reserves a funded change box and selects an explicit ERG-only funding input', () => {
  const unfunded = plan([{ tokenId: keyA, amount: '1', erg: '0.001' }], 1_000_000)
  assert.equal(unfunded.fundingNeededNano, 2_100_000)
  assert.notEqual(unfunded.error, null)
  const sources = [mixed, box('small', '2000000'), box('enough', '5000000'), box('large', '9000000'), box('other-tokens', '2500000', [{ tokenId: keyB, amount: '1' }])]
  assert.deepEqual(findIsolationFunding(sources, new Set(['mixed']), unfunded.fundingNeededNano), ['enough'])
  assert.deepEqual(findIsolationFunding(sources, new Set(['mixed', 'enough', 'large']), 3_000_000), [])
  const funded = plan([{ tokenId: keyA, amount: '1', erg: '0.001' }], 6_000_000)
  assert.equal(funded.error, null)
  assert.equal(funded.changeNano, 3_900_000)
})

test('rejects dust change and permits exactly balanced outputs without change', () => {
  const result = plan([{ tokenId: keyA, amount: '1', erg: '0.001' }], 2_600_000)
  assert.equal(result.fundingNeededNano, 500_000)
  const single = [tokens[0]]
  const balanced = planTokenIsolation(single, [{ tokenId: keyA, amount: '1', erg: '0.001' }], 2_100_000, 1_100_000, 1_000_000, 120)
  assert.equal(balanced.error, null)
  assert.equal(balanced.remainingTokens.length, 0)
  assert.equal(balanced.changeNano, 0)
})

test('aggregates only selected boxes and preserves exact totals', () => {
  const result = collectIsolationTokens([mixed, box('extra', '1000000', [{ tokenId: fungible, amount: '7' }])], [])
  assert.equal(result.find(t => t.tokenId === fungible)?.rawAmount, '9007199254741000')
  assert.equal(collectIsolationTokens([], []).length, 0)
})

test('adds an exactly balancing funding input without reserving unnecessary change', () => {
  const keyOnly = collectIsolationTokens([box('key', '1000000', [{ tokenId: keyA, amount: '1' }])], [])
  const deficit = planTokenIsolation(keyOnly, [{ tokenId: keyA, amount: '1', erg: '0.001' }], 1_000_000, 1_100_000, 1_000_000, 120)
  assert.equal(deficit.exactFundingNano, 1_100_000)
  assert.equal(deficit.fundingNeededNano, 2_100_000)
  const sources = [box('exact', '1100000'), box('with-change', '5000000')]
  assert.deepEqual(findIsolationFunding(sources, new Set(['key']), deficit.fundingNeededNano, deficit.exactFundingNano), ['exact'])
  const funded = planTokenIsolation(keyOnly, [{ tokenId: keyA, amount: '1', erg: '0.001' }], 2_100_000, 1_100_000, 1_000_000, 120)
  assert.equal(funded.error, null)
  assert.equal(funded.changeNano, 0)
  const leftovers = plan([{ tokenId: keyA, amount: '1', erg: '0.001' }], 1_000_000)
  assert.equal(leftovers.exactFundingNano, null)
  assert.deepEqual(findIsolationFunding([box('too-small', '1100000')], new Set(), leftovers.fundingNeededNano, leftovers.exactFundingNano), [])
})

test('combines exactly balancing funding inputs if no sufficient single box exists', () => {
  assert.deepEqual(findIsolationFunding([box('a', '1000000'), box('b', '1000000')], new Set(), 3_000_000, 2_000_000), ['b', 'a'])
})
