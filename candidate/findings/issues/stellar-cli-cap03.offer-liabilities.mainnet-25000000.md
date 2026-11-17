<!-- repo: stellar/stellar-cli -->
# stellar-cli 28.1.0, 30.0.1: the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported (cap03.offer-liabilities.mainnet-25000000)

## What happens

On the input below, `stellar-cli` 28.1.0, 30.0.1 answers differently from the Stellar network in 28 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap03.offer-liabilities.mainnet-25000000` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `mainnet/tx_hash 0` on 28.1.0, 30.0.1

the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported

```json
{
  "depth_limit": 1000,
  "len_limit": 272,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAABnsfj1ag4cLtRF2vYdcIxmkA7L6p2nH3Te634UwjXenAAAAZAFqIVwAAL1RAAAAAQAAAAAAAAAAAAAAAF05VRoAAAABAAAAD3RvcHVwIDg0NDM4Nzg1NAAAAAABAAAAAAAAAAEAAAAA4gMhe5+MoQ8n9qcbShIuP2IuF5+FzhlaCTkGmxMkpYMAAAABRFJBAAAAAACSqQIEEE0lvhYFPsJxdYVz5AxNoDraze8VLZsFvaBKywAAAACDIVYAAAAAAAAAAAEwjXenAAAAQHA6y3hqMQlbQ2293dK9bKKFyhSirSA+C0fBuV6Dc7tf5ZvE7MSb+FWfx/6zfd7AvnESty7bgnwg5/D+hWIE+gY="
}
```

Expected: `{"hash":"34289378a76b37eb3e4be6b9052cd44fd4902e3c35d67d5790b22c4c992f7fc3"}`

Actual: `{"rejected":"❌ error: only transaction v1 is supported"}`

### `mainnet/tx_hash 1` on 28.1.0, 30.0.1

the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAABlOzgWjBmqrEHvGIemKVSliLlABFNbTkSwFYsohekGmAAAAZAFiuIoABUJJAAAAAAAAAAAAAAABAAAAAAAAAAMAAAAAAAAAAVhSUAAAAAAAywVQu7fTri1EifciQSufiYTCAFfd0oD9SHt6cpmFIEAAAAAAAiuG8wCYloABOTeZAAAAAAAAAAAAAAAAAAAAASF6QaYAAABAzZiKj2vMxCYwUgq26T/mp1YkdRJhtALopXUiBKrYzWPvL+0uXCLJUbCdTi++iVSAfyGiHPLwvGuKE/ZoI2trAA=="
}
```

Expected: `{"hash":"0f4951728623d2d35cd3e114dce017305b35ef584756586d99e3a0799db6685f"}`

Actual: `{"rejected":"❌ error: only transaction v1 is supported"}`

### `mainnet/tx_hash 2` on 28.1.0, 30.0.1

the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported

```json
{
  "depth_limit": 1000,
  "len_limit": 236,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAJAovADXbHWSCERfsKVGsCp4LOpEG6k0QgtRlbHVJGo/AAAAZAF51R8AAFHrAAAAAQAAAAAAAAAAAAAAAF05VQMAAAAAAAAAAQAAAAAAAAADAAAAAAAAAAFTTFQAAAAAAJQPK68UPPaXsYBL98BptE7vUcYrIrVseDjcjXOV5+6oAAAAAf6Gu5AAvMcxCXRTUAAAAAAGM3ahAAAAAAAAAAHVJGo/AAAAQF4bCxNa2bIQimu6c8q5jo4+YZ15GF++bsjEsvTXhZ5S8dR59AKPji8z2DmWN8S+y+RrAEAc+nnMqHhcFD7WHQw="
}
```

Expected: `{"hash":"d6bd290dcc1156afa389140340f591cbf83cbc8a86c9fd1ec700e9bb36f7bb56"}`

Actual: `{"rejected":"❌ error: only transaction v1 is supported"}`

### `mainnet/tx_hash 3` on 28.1.0, 30.0.1

the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAM6uNwIhKHsbR06lpCfWSJ5D8CXSpx1Yl6TCndjWqED2AAAAZAEoFloAD+3JAAAAAAAAAAAAAAABAAAAAAAAAAMAAAAAAAAAAUdUTgAAAAAAIlYA8GcSNHeyVopiPW7NPhmRw5bx2DZ7qyEEU3aujzAAAAAACOEz/QAbFxMATEtAAAAAAAAAAAAAAAAAAAAAAdaoQPYAAABAIm/Hwol72mi6O1rwWidIABvkGEjE650hufOMwtCeEVMiMyP2d58YDjq4xmo7PBGG9hTj3zm+KreHksm3Zql8Cw=="
}
```

Expected: `{"hash":"430f6e973387cad59a8e03843bc207b8e473d22c05670a16ee0fa6ce951d256e"}`

Actual: `{"rejected":"❌ error: only transaction v1 is supported"}`

### `mainnet/tx_hash 4` on 28.1.0, 30.0.1

the sdk refused a value the network accepts: ❌ error: only transaction v1 is supported

```json
{
  "depth_limit": 1000,
  "len_limit": 308,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAFAzDlWFwHNlriTl8A6MsIcoJpyPdkzFXnNfsIOaolfXAAAnEAD1qIoAAg0XAAAAAAAAAAAAAAABAAAAAQAAAAAxzt2Pk+NS6cvKaw3kOcjrNZ0BxObvSLC3mmJysAO15AAAAAUAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAlsb2JzdHIuY28AAAAAAAAAAAAAAAAAAAKaolfXAAAAQIt7JzVdU6IOjNmtELgyO8eBB7ykH2ER67ihFJ1Uq4ceHG3lDbaku8AnqhfdU7nLduy03H79xEPKE6n7QB2B6QOwA7XkAAAAQANK0qoQ82TupBg3NNRREv0LBohuv7JcP+R3aDnzl29V3Tweh6eW9ZLQ0uQsROaZLbJjyEavNF5Z6bqzFoqQegg="
}
```

Expected: `{"hash":"4159f0d5ead1369a8db9cbad2ee1a1176ff29b85a38cdfb7d40383114203b7f3"}`

Actual: `{"rejected":"❌ error: only transaction v1 is supported"}`

9 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk stellar-cli --version 28.1.0 --entry cap03.offer-liabilities.mainnet-25000000
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 1 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 10 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 11 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 12 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 13 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 2 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 3 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 4 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 5 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 6 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 7 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 8 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-25000000/mainnet.xdr,) envelope 9 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: captured 2026-11-13 through the sep-54 public data lake from https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet/FE8323FF--24960000-25023999/FE8287BF--25000000.xdr.zst. ledger 25000000 hash 98c64c37e20ba4bb3c7c225b99f17080c06f625d23f570008efc81cb5107c727, protocol 11, LedgerCloseMeta v0. the public lake holds history as re-emitted by a current core (TransactionMeta v4 for every protocol). the header's scp value is STELLAR_VALUE_BASIC, unsigned: current core's HerderImpl::makeStellarValue writes only signed values, and mainnet headers carry basic values through protocol 11 (sampled at ledgers 19500000 to 25500000)

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap03.offer-liabilities.mainnet-25000000`
