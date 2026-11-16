<!-- repo: stellar/stellar-etl -->
# Question: no `trustline_created` effect for a trustline opened by a Stellar asset contract `trust()` call

Checked at [2b24ec89](https://github.com/stellar/stellar-etl/commit/2b24ec89ddce7fc0beb7795de982c93e6895eaf3) (v2.8.34).

## What we see

Since protocol 26 ([CAP-73](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0073.md)) an account can open a trustline by calling `trust()` on a Stellar asset contract. The ledger then holds a new trustline entry, created by an `InvokeHostFunction` operation.

The effects table has no `trustline_created` row for it. A `ChangeTrust` operation that opens the same trustline gets one ([`addChangeTrustEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L647-L705)).

## Where it comes from

For `InvokeHostFunction`, [`addInvokeHostFunctionEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1326-L1440) reads only the Stellar asset contract's `transfer`, `mint`, `clawback` and `burn` events and turns them into credit and debit effects. `trust()` emits none of these, and the ledger entry changes of the operation are not read for effects.

## Question

Is the effects table meant to describe only the token movements of a contract call, or should a trustline the call creates get `trustline_created` as it does for `ChangeTrust`? If the omission is intended, a line in the effects documentation would let consumers know to read the trustline from the ledger entry changes instead.

## Reproduction

Testnet passphrase, standalone stellar-core 30.0.0, ledger 71 of the `cap73.sac-trust.p30` chain, transaction `1c7551b720df08914b37b0e501b2795df68587876502883f34fad0135f365d13`: `GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ` calls `trust()` on the contract of `TRST:GBRHNLJWCOTZPE45Z33CUBA335WGS25NNAUH5VVBFBGNTKVDA32LLZA5`, and the meta creates that trustline with the limit `9223372036854775807`. The effects export of that ledger has no row for the trustline.
