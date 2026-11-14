<!-- repo: stellar/wallet-backend -->
# Comment for #804: two more ledgers that reproduce it, at protocol 28 and 30

Two more inputs, in case they help as regression fixtures. Both come from standalone stellar-core 30.0.0 networks under the testnet passphrase: one network runs protocol 28, the other protocol 30. In each, `GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ` sends 100 XLM through the native SAC to `GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM`, which did not exist, and the operation meta creates its account entry:

- protocol 28, ledger 69, transaction `f715fe4d1651ded84dd1d5656f5e1c1b7fa17f5236dcc339b04eff051d1dd371` ([Galexie file](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/chains/cap73.sac-trust.p28/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFBA--69.xdr.zst));
- protocol 30, ledger 73, transaction `69d6a941d953100889d67b93301a822edc56557edb67abe24be12f8ce624311a` ([Galexie file](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/chains/cap73.sac-trust.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFB6--73.xdr.zst)).

Run through `ProcessLedger` at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (go1.27.4), each gives the same result as in the issue: the fee debit, the transfer's debit and credit, and no `ACCOUNT` `CREATE`:

```
=== RUN   TestSACTransferCreatesAccountCorpus
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p28: BALANCE DEBIT account=GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ amount=101419
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p28: BALANCE DEBIT account=GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ amount=1000000000
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p28: BALANCE CREDIT account=GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM amount=1000000000
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p30: BALANCE DEBIT account=GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ amount=6851
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p30: BALANCE DEBIT account=GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ amount=1000000000
    sac_transfer_creates_account_test.go:28: sac_transfer_creates_account_p30: BALANCE CREDIT account=GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM amount=1000000000
--- PASS: TestSACTransferCreatesAccountCorpus (0.00s)
```

The files are from the [shapekit](https://github.com/ErenTopaal/shapekit) corpus, which also has mainnet-passphrase variants of both chains.
