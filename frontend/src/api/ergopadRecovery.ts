import { invoke } from '@tauri-apps/api/core'

export interface StakeStateSnapshot {
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
  stakeKeyId: string
  stakeBoxId: string
  stakeBoxValueNano: number
  ergopadAmountRaw: number
  checkpoint: number
  stakeTimeMs: number
  ergopadAmountDisplay: string
}

export interface RecoveryScan {
  state: StakeStateSnapshot
  stakes: RecoverableStake[]
  candidatesChecked: number
  boxesScanned: number
  pagesFetched: number
  hitPageLimit: boolean
}

export async function scanErgopadRecoverableStakes(
  candidateTokenIds: string[],
): Promise<RecoveryScan> {
  return await invoke<RecoveryScan>('scan_ergopad_recoverable_stakes', {
    candidateTokenIds,
  })
}

export async function previewErgopadRecovery(
  stakeKeyId: string,
): Promise<RecoverableStake> {
  return await invoke<RecoverableStake>('preview_ergopad_recovery', { stakeKeyId })
}

export async function buildErgopadRecoveryTx(
  stakeKeyId: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('build_ergopad_recovery_tx', {
    stakeKeyId,
    userUtxos,
    currentHeight,
  })
}
