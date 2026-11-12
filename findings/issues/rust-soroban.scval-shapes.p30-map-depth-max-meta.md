<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3679 (soroban.scval-shapes.p30-map-depth-max-meta)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-max-meta` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-max-meta` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3679

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999829808","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99989674935","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}}],"post_tx_apply_fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99989674935","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999693689","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"2... (17314 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAABGVHnwWUOIbIDDAxhKlzoFjv7vA+gIr0l2W1LqsFDLpAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 3679"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 3679

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999829808","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99989674935","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}}],"post_tx_apply_fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99989674935","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999693689","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"2... (17314 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAANyh/t/jl21GMrrklhaN1HpwfQCRTTDrPyHmTB+PqdKIAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 3679"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-map-depth-max-meta
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.xdr

Entry notes: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-meta`
