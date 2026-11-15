# indexerlint

`indexerlint` checks Go indexer code built on `github.com/stellar/go-stellar-sdk` for the three ingestion mistakes the [indexer-hardening guide](guide/indexer-hardening.mdx) describes:

| Rule | Name | What it reports |
|---|---|---|
| IDX001 | unvalidated-field-trusted | a transaction field stellar-core does not validate is used as a key, stored, or converted lossily without a check |
| IDX002 | missing-depth-or-size-cap | a recursive xdr structure is walked, or a decoded length is allocated, without a cap; or xdr is decoded with the depth limit off |
| IDX003 | participants-from-envelope | participants are derived from the transaction envelope instead of meta changes or events |

It is a static analyzer built on `golang.org/x/tools/go/analysis`. It matches go-stellar-sdk types by import path and name, so it works on code pinned to any go-stellar-sdk release that keeps those names. The wallet-backend results below cover code pinned to v0.7.3 and to v0.8.0.

## Running it

Build:

```
cd lint && go build -o indexerlint ./cmd/indexerlint
```

Standalone, from the root of the module to check:

```
indexerlint ./...
indexerlint -format json ./...
indexerlint -baseline indexerlint-baseline.json ./...
```

Exit codes: `0` no findings, `1` findings, `2` usage, load or analysis error (including an invalid baseline). Text output is one line per finding, `file:line:col: RULE: message [function]`, then one line per baseline entry that matched nothing, then a count line. JSON output has `findings`, `suppressed`, `baselined`, `unused_baseline_entries` and `counts`. Without `-baseline`, the standalone driver reads the baseline path from the `INDEXERLINT_BASELINE` environment variable.

Under `go vet`:

```
go vet -vettool=$(command -v indexerlint) ./...
```

Suppressions and the baseline apply in both modes. Under `go vet` the baseline is passed through the `INDEXERLINT_BASELINE` environment variable (an absolute path), and vet prints only the findings, not the suppressed and baselined counts. Test files (`_test.go`) are not checked in either mode.

## Suppressing a finding

A directive on the finding's line, on the line above it, or in the doc comment of the enclosing function:

```go
//indexerlint:ignore IDX002 the value comes from the service configuration, never from ledger data
```

The rule ID and a reason are both required. A directive without them, or with an unknown rule ID, suppresses nothing and is itself reported as a finding (rule `directive` in the standalone output). To name several rules, separate them with commas: `IDX001,IDX003`. Suppressed findings are counted, and the JSON output lists them with their reason.

IDX003 reports once per function, at the first place an envelope address enters an account set. IDX001 reports once per function and field. A directive on the reported line therefore suppresses that whole finding.

## Baseline

A baseline accepts known findings that are tracked upstream:

```json
{
  "version": 1,
  "entries": [
    {
      "rule": "IDX002",
      "package": "github.com/stellar/wallet-backend/internal/indexer/processors",
      "function": "(*ContractDeployProcessor).ProcessOperation",
      "issue": "https://github.com/stellar/wallet-backend/issues/<number>",
      "note": "optional free text"
    }
  ]
}
```

Entries match by rule, package and function (`Name`, `(T).Name` or `(*T).Name`), not by line, so unrelated edits do not invalidate them. An optional `message` must match exactly. `package` and `function` are required, and every entry needs an `https://` issue link; a baseline that breaks either rule, has another `version` or has unknown fields is rejected. Entries that matched nothing are listed, so you can remove the entry of a fixed finding.

## IDX001: unvalidated field trusted

The fields come from `registry/fields.toml`, the same file that produces the guide's field table. `go generate ./internal/fieldtable` in `lint/` turns it into `lint/internal/fieldtable/table_gen.go`, and a test fails when the generated table and the registry differ, so the linter and the guide cannot drift.

Fields checked: every row with `validated_by_core = "no"` (mux ids of every muxed account, memo text, id, hash and return hash, manage data values), plus the `partial` rows whose checked part is not the part an indexer relies on: the signed payload of a signer key, the declared Soroban footprint, and declared auth entries with their credentials, nonces, signatures, delegates and invocation trees. A read of any `go_accessors` entry of those rows starts a flow. For mux ids the accessors are `MuxedAccount.Med25519`, `GetMed25519`, `MustMed25519`, `GetId` and `MuxedAccountMed25519.Id`. A test (`TestAccessorsExistInSDK`) resolves every listed accessor against the go-stellar-sdk the testdata module pins (v0.8.0). Setting `IDX001_SDK_MODULE` to another module directory runs the same check against another release; it passes against v0.9.0. Three methods are not listed as accessors but carry a field inside their result and are treated as accessors of the matching rows: `MuxedAccount.Address` and `MuxedAccount.GetAddress` (an M strkey carries the mux id) and `SignerKey.Address` (a P strkey carries the payload).

A finding is reported when that value, inside the function, reaches:

- a key: a map store (`m[k] = v`, `m[k] += v` or `m[k]++`), a set `Add`/`Store`/`Put`/`Insert`/`Union`, or a `New...Set(...)` constructor;
- storage: a struct field with a `db` tag, a SQL `Exec`/`Query`/`QueryRow`/`CopyFrom`/`Queue` call, or an argument of a function whose parameter reaches storage (worked out per function and passed across packages as an analysis fact);
- a lossy conversion: bytes to `string`, or an unsigned 64-bit value to a signed integer.

Keys and storage are checked for strings, byte slices and arrays, and named struct types. Integers are checked there only when they carry a mux id. A mux id is any 64-bit value the submitter picked. Code that uses it as a map key, or stores it, without first looking at the account type treats a number with no account behind it as an identity (and `id, _ := acct.GetId()` stores 0 for every account that is not muxed). Once the code has looked at the type, storing the id as a `uint64` next to the base account is the handling the registry asks for, and it is not reported. Other numbers, such as a memo id, are only checked at conversions.

Recognized checks: `utf8.Valid`, `utf8.ValidString` or `strings.ToValidUTF8` on the value; for mux ids, a comparison or switch on the muxed account's `Type`, an `if` on the `ok` of `GetMed25519` or the error of `GetId`, or `ToAccountId()`, which drops the id; for declared auth and footprint data, an equality comparison or a map lookup in a function whose call tree reads transaction meta (the pattern `if _, ok := created[contractID]; ok`).

What it cannot see: flows through struct fields or interface maps between functions (a value put into `map[string]interface{}` in one function and read back in another is lost), flows through channels and goroutines, a check done in a caller, `ScAddress.String()` on a muxed account address (left out because nearly every call is on a contract address), and whether a check is the right one (any `utf8.ValidString` call on the value clears it, whatever the code does with the result; a look at the account type clears a mux id even when the code then keys a map by the id alone).

## IDX002: missing depth or size cap

Three checks:

1. Recursion. A function, or a closure assigned to a variable, that recurses directly or through other functions and closures of the same package, and takes one of `xdr.ScVal`, `xdr.ScVec`, `xdr.ScMap`, `xdr.ScMapEntry`, `xdr.SorobanAuthorizedInvocation`, `xdr.SorobanDelegateSignature` or `xdr.ClaimPredicate` (directly or behind pointers, slices or maps). It is reported unless some integer that survives from call to call (a parameter, a captured variable or a struct field) is both compared and stepped in the cycle: `walk(v, depth+1)` with `if depth > max`, `remaining-1` with `if remaining == 0`, or `n++` with `if n > max`. Locals of the recursive function, such as loop indexes, do not count.
2. Allocation. `make` whose length or capacity comes from an integer of a named xdr type (`xdr.Uint32`, `xdr.Int64`, and so on, including values read through conversions and local variables) with no ordered comparison on it in the same function and no `min(...)` around it. Sizes from `len()` of something already decoded are not reported: the decoder already bounded them by the input.
3. Decoding. go-stellar-sdk's `SafeUnmarshal`, `SafeUnmarshalBase64`, `UnmarshalBinary`, `Unmarshal` and go-xdr's `NewDecoder` all apply a default depth limit of 1500 and cap slice preallocation at 256 elements, and the base64, hex and byte-slice helpers set the input length. Decoding with the defaults is therefore not reported. What is reported is turning the depth limit off: any use of `xdr3.DecodeUnlimitedDepth`, a `DecodeOptions` with `MaxDepth` set to the largest `uint`, or `DecodeFrom(d, maxDepth)` called with it. A finite `MaxDepth` above the default is not reported.

What it cannot see: recursion through interface methods, method values or function values passed as arguments; recursion across packages; a bound that is compared but never stops the recursion (the direction of the comparison is not checked); a node-count cap implemented through a slice length rather than an integer; allocations sized through a function call that does not bound its input (calls other than conversions stop the flow); `MaxMemoryBytes`, which go-xdr leaves unlimited by default (its preallocation cap keeps memory proportional to the input).

## IDX003: participants from envelope

A function is reported when it puts an address read from the envelope into an account set and neither it nor any function it calls statically reads transaction meta.

Envelope accessors: transaction, fee bump and operation source accounts; destinations of create account, payment, path payments, account merge and claimants; allow trust and set trust line flags trustors; the sponsored account; the clawback source; revoke sponsorship ledger keys and signers; set options signers and inflation destination; Soroban auth credential addresses (including CAP-71 delegates); declared contract addresses and arguments of invocations; contract id preimages of declared deployments; footprint keys; and `ingest.LedgerTransaction` helpers that return the fee or source account. The value is followed through assignments, range loops, field reads, conversions, composite literals, method calls on it, calls it is passed to, and functions whose result is envelope-derived (also across packages, through facts).

Account sets: the return value of a function named for participants or accounts when it is a collection of strings or xdr account types; appends, map stores and `Add`/`Union`/`Insert`-style calls on such a collection inside such a function, or on a variable or field named for participants or accounts; struct fields named for participants or accounts (but not fields of xdr values, such as building a `Transaction` to hash); arguments of `With`/`Set`/`Add` methods whose name mentions an account, participant, creator or deployer, as on a state change builder. A helper that only fills a set passed in by its caller is judged at the call site, so a helper called from a function that also reads meta is not reported.

Reading meta means touching a value of a meta type: `TransactionMeta` and its versions, `OperationMeta`, `LedgerEntryChange(s)`, `SorobanTransactionMeta`, contract, diagnostic and transaction events, `TransactionResultMeta`, `ingest.Change`, `ingest.TransactionEvents`, or any type of the `processors/token_transfer` package. Callees in other packages are followed through facts.

What it cannot see: a meta read that does not actually confirm the envelope address (any meta read in the call tree clears the function; the rule checks that meta is consulted, not that it is consulted correctly); account sets that are neither named for participants or accounts nor returned from such a function; calls through interfaces or function values; envelope addresses that reach a set through struct fields set in another function. A source account taken from the envelope is reported like any other envelope address when no meta is read. The fee source is always charged, and the fee changes in meta show it, so reading `FeeChanges` clears it.

## Results on stellar/wallet-backend

Checked at the commits below with `indexerlint ./...`; `go vet -vettool` at the current pin gives the same finding. Findings are per function. Outside its tests and integration test helpers, wallet-backend does not read mux ids through the xdr accessors: it takes the id from the token transfer processor's `MuxedInfo` and reduces envelope accounts with `ToAccountId()` before using them, so the mux-id key and storage checks have nothing to report there.

| Commit | Role | IDX001 | IDX002 | IDX003 |
|---|---|---|---|---|
| `2a0a84ea` | before PR #729 | 7 | 2 | 7 |
| `79ea2acd` | before PR #693, issue #716 | 7 | 2 | 7 |
| `f8e5786d` | before PR #753 | 7 | 2 | 7 |
| `666a64d4` | before `9538b3ca` (auth-tree participants) | 3 | 2 | 7 |
| `9538b3ca` | before `80121f3a` (issue #746) | 1 | 1 | 1 |
| `80121f3a` | after the #746 fix | 0 | 1 | 0 |
| `fbdb593e` | before PR #757 | 0 | 1 | 0 |
| `e967ce8b` | previous pin | 0 | 1 | 0 |
| `3e22515a` | current pin | 0 | 1 | 0 |

- #716 and #753: IDX001 reports the four sites in `contract_operations.go` that seed Soroban participants with `SourceAccount().Address()`, the producer #716 names first and #753 fixed; the findings disappear at `666a64d4`. The other two producers in #716 (a SEP-41 `ScAddress.String()` and a signed-payload signer) are not reported; see the IDX001 limits.
- #729: not reported. The P strkey travels from the effects code to the state change builder through a `map[string]interface{}`, which IDX001 does not follow.
- Auth-tree participants (fixed in `9538b3ca`): at `666a64d4`, IDX003 reports six of its seven functions (the seventh is #746), IDX001 the credential address and the declared contract IDs used as set keys, and IDX002 the unbounded tree walk in `participantsFromInvocationAndSubInvocations`.
- #746 (fixed in `80121f3a`): IDX003 and IDX001 report `ContractDeployProcessor.ProcessOperation` at `9538b3ca`; both clear once declared deployments are checked against created contract instances in meta.
- #757 adds a per-transaction row limit to a SQL query on the serving side. That is a query change, not a pattern in Go source, so it is outside the three rules.
- Current pin (`3e22515a`, same findings as `e967ce8b`; the two commits between them only touch Blend decoding and serving): one finding, IDX002 on the `walkInvocation` closure in `ContractDeployProcessor.ProcessOperation`, which walks the declared auth invocation tree with no depth limit. It is a true positive under the rule. Its practical reach is bounded by the decoder (depth 1500 by default) and by core, which rejects transactions nested deeper than 1000 from protocol 28, so it is a hardening finding, not a halt. It is filed upstream as [wallet-backend#820](https://github.com/stellar/wallet-backend/issues/820) and baselined in [`lint/baselines/wallet-backend.json`](../lint/baselines/wallet-backend.json), so `indexerlint -baseline lint/baselines/wallet-backend.json ./...` on wallet-backend at the pin exits 0 with 0 findings and 1 baselined. The nightly workflow runs that check.

## Seeded tests

Each rule has a testdata module under `lint/<rule>/testdata` that imports the real go-stellar-sdk v0.8.0. Seeded violations carry `// want` expectations; clean counterparts must produce nothing. The testdata includes reduced copies of the wallet-backend code behind #716, #753, #746 and the auth-tree participants fix, with the upstream file and commit in a comment.

| Rule | Seeded violations | Detected | Unexpected findings |
|---|---|---|---|
| IDX001 | 15 | 15 | 0 |
| IDX002 | 14 | 14 | 0 |
| IDX003 | 11 | 11 | 0 |

On the seeded set, precision and recall are both 1.0. That shows the rules do what their tests specify; the wallet-backend runs above measure precision on real code.
