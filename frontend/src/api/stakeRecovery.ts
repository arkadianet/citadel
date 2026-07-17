import { invoke } from '@tauri-apps/api/core'

export interface StakeStateSnapshot {
  protocol: string
  stateBoxId: string
  stateBoxValueNano: number
  totalStakedRaw: number
  checkpoint: number
  numStakers: number
  lastCheckpointTs: number
  cycleDurationMs: number
  stakeTokenAmount: number
}

export interface RecoverableStake {
  protocol: string
  rewardTokenName: string
  stakeKeyId: string
  stakeBoxId: string
  stakeBoxValueNano: number
  rewardAmountRaw: number
  checkpoint: number
  stakeTimeMs: number
  rewardAmountDisplay: string
}

export interface RecoveryScan {
  states: StakeStateSnapshot[]
  stakes: RecoverableStake[]
  candidatesChecked: number
  boxesScanned: number
  pagesFetched: number
  hitPageLimit: boolean
}

export async function scanRecoverableStakes(
  candidateTokenIds: string[],
): Promise<RecoveryScan> {
  return await invoke<RecoveryScan>('scan_recoverable_stakes', {
    candidateTokenIds,
  })
}

export async function previewRecovery(
  stakeKeyId: string,
): Promise<RecoverableStake> {
  return await invoke<RecoverableStake>('preview_recovery', { stakeKeyId })
}

export async function buildRecoveryTx(
  stakeKeyId: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('build_recovery_tx', {
    stakeKeyId,
    userUtxos,
    currentHeight,
  })
}

// ----- Paideia two-step unstake (step 2 = permissionless payout / refund) -----

export interface DryRunResult {
  valid: boolean
  message: string
}

export interface PaideiaProxyCheck {
  proxyBoxId: string
  executor: DryRunResult
  refund: DryRunResult
}

/** Resolve the proxy box (output[0]) created by a confirmed step-1 tx. */
export async function paideiaProxyBoxId(txId: string): Promise<string> {
  return await invoke<string>('paideia_proxy_box_id', { txId })
}

/** Dry-run BOTH proxy spend paths via the node's /transactions/check (no broadcast). */
export async function checkPaideiaProxy(
  proxyBoxId: string,
): Promise<PaideiaProxyCheck> {
  return await invoke<PaideiaProxyCheck>('check_paideia_proxy', { proxyBoxId })
}

/** Broadcast one proxy spend path: 'executor' (pay out reward) or 'refund'. */
export async function submitPaideiaProxyTx(
  proxyBoxId: string,
  which: 'executor' | 'refund',
): Promise<string> {
  return await invoke<string>('submit_paideia_proxy_tx', { proxyBoxId, which })
}
