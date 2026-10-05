NETA mainnet fixtures fetched on 2026-09-28:

- Stake key: `12fb8192304383f32a5eb880b3e32536036e9ef9150137555ebdde25d03943d0`
- [Stake box](https://api.ergoplatform.com/api/v1/boxes/4c1d085f79ea14a9fcf821b2e96077de3a7f7121d2dc261b72d701659c79e5d5): 2,386.034991 NETA, checkpoint 182.
- [State box](https://api.ergoplatform.com/api/v1/boxes/5df4fdabdfb32f4c10afabfe94df057e5ebeaaefdeee05efb340b748d7043c4b): checkpoint 182, 495 stakers.
- `headers.json`: ten consecutive mainnet headers ending at height 1,882,943, fetched from `https://node.ergo.watch/blocks/lastHeaders/10`.

Box fixtures preserve the original transaction IDs, indices, values, scripts,
tokens and registers; explorer metadata is omitted and registers use node JSON
format. The integration test parses these boxes and reduces the recovery through
the actual state and stake scripts, asserting both authorize it. The wallet key
still requires its owner's signature. A synthetic funding box covers the tiny
39,960 nanoERG key box; nothing is signed or broadcast.

NETA uses a different StakeBox from both Ergopad and Paideia. Its full unstake can
use a direct transaction: state input 0, stake input 1, key input 2 (NFT at token
index 0); updated state output 0, full NETA payout output 1, NFT returned in change.
The stake must be at the state checkpoint. No proxy, operator or key burn is
required for this tested direct path.

The zero-confirmation regression uses a local mock node serving the old confirmed
state plus a pending redemption. It builds a second claim with a distinct
synthetic key/position under the real NETA scripts, spending the first claim's
state and wallet change. Both claims are reduced locally; no transaction is
broadcast. The mock also reverses pending transaction order, repeats a transaction,
places the relevant transaction on the second mempool page, and returns failed or
malformed mempool responses. Discovery must hide spent stakes and state selection
must find the last unspent state or return an error.

# Paideia recovery fixtures

`paideia_state.json` and `paideia_stake.json` are the first two input boxes of
the accepted mainnet unstake transaction
[`fccb0c4979d43295a27af7be0da3aa93db840776979621bee7c13a1b51c3cf97`](https://api.ergoplatform.com/api/v1/transactions/fccb0c4979d43295a27af7be0da3aa93db840776979621bee7c13a1b51c3cf97),
read on 2026-10-05. Explorer creation metadata and register `serializedValue`
fields were normalized into node box JSON. The Paideia tests create a new proxy
at the current header height, reduce both payout and refund scripts before any
confirmation, then spend the first payout's replacement state in a second claim.
The additional stake position uses a synthetic key with the same real script.
