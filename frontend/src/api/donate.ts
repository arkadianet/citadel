/**
 * Voluntary tip / “Buy me a coffee” — simple ERG send to the Citadel fee address.
 */

import { DEFAULT_DEV_FEE_ADDRESS } from '../constants'
import {
  buildSendTx,
  startSign,
  getTxStatus,
  type SendBuildResponse,
} from './wallet'

export { DEFAULT_DEV_FEE_ADDRESS }
export type { SendBuildResponse }

export async function buildDonationTx(params: {
  changeAddress: string
  /** tip amount in nanoERG as decimal string */
  ergNano: string
  userUtxos: object[]
  currentHeight: number
}): Promise<SendBuildResponse> {
  return buildSendTx({
    recipientAddress: DEFAULT_DEV_FEE_ADDRESS,
    changeAddress: params.changeAddress,
    ergNano: params.ergNano,
    userUtxos: params.userUtxos,
    currentHeight: params.currentHeight,
  })
}

export { startSign, getTxStatus }
