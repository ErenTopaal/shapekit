<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3676 (soroban.scval-shapes.p30-vec-depth-max-meta)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-vec-depth-max-meta` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-vec-depth-max-meta` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3676

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999972362","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":70}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999811658","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}}],"post_tx_apply_fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999811658","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999836019","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"2... (7776 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAGEHz0thxwyNqEav2wfA+0OPMJHTt2v4NKuDMoDxNfA1AAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAANu53zfcQUeSGDSRB1fEPAq5TLpMp2GIwealAdu5RjuFAAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHZ8CgAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0CEoAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 3676"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3676

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999972362","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":70}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999811658","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}}],"post_tx_apply_fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999811658","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999836019","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"2... (7776 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAEZZqjw4dH8dmtYy9wo7S53/NzS0RiR7pUovyiAY0S6/AAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAAEi1Dv+0WZFxCxAoHq9sayasjIaJoL+BSCZcyLpjxMb5AAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHZ8CgAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0CEoAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 3676"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-vec-depth-max-meta
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/testnet.xdr

Entry notes: 99 nested vecs around a u32 leaf (100 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-max-meta`
