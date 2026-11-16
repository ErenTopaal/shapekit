<!-- repo: stellar/stellar-etl -->
# Question: no `account_created` effect for an account a Stellar asset contract transfer creates

Checked at [2b24ec89](https://github.com/stellar/stellar-etl/commit/2b24ec89ddce7fc0beb7795de982c93e6895eaf3) (v2.8.34).

## What we see

Since protocol 26 ([CAP-73](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0073.md)) an XLM transfer through the native asset contract to an account that does not exist creates that account. The effects table records the transfer as `account_debited` and `account_credited`, but has no `account_created` row for the new account.

## Where it comes from

[`addInvokeHostFunctionEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1326-L1440) builds the effects of a contract call from its Stellar asset contract events only. `account_created` is written only for a `CreateAccount` operation ([`addAccountCreatedEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L411-L437)).

## Question

Should an account created by a contract transfer get `account_created`, as one created by `CreateAccount` does? If not, documenting that the effects of a contract call cover token movements only would tell consumers to find new accounts in the ledger entry changes.

## Reproduction

Testnet passphrase, standalone stellar-core 30.0.0, ledger 73 of the `cap73.sac-trust.p30` chain, transaction `69d6a941d953100889d67b93301a822edc56557edb67abe24be12f8ce624311a`: `GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ` transfers 100 XLM through the native asset contract to `GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM`, which did not exist, and the meta creates it with a balance of 1000000000 stroops. The effects export of that ledger has `account_debited` and `account_credited` rows but no `account_created`.
