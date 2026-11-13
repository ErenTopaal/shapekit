package walletbackend

import (
	"encoding/hex"
	"encoding/json"
	"maps"
	"os"
	"strings"
	"testing"

	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"
)

const (
	testPassphrase = "Test SDF Network ; September 2015"
	// the account and signed-payload signer of cap40.signed-payload-signer.p28-set-options-payload-32
	testAccount = "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2"
	testSigner  = "PDDLGS6XTXODPNKK7OP4FHZSNGVJG3OE3GKGMYN47OP3PWM3SSTPCAAAAAQLTTLAOIMJNKCNM54J2UYSG6QYOCGN2XAXHXGHLYGIRPJKRQHFSQH534"
	testIssuer  = "GD32JQHFWLIROCNQ564GLCB2M7T4GMQAP5UKHMVT64OQISWL2LHYK2HO"
)

// stored renders a strkey the way wallet-backend's fixed commits store it:
// version byte, then the whole payload.
func stored(t *testing.T, s string) []byte {
	t.Helper()
	v, payload, err := strkey.DecodeAny(s)
	if err != nil {
		t.Fatal(err)
	}
	return append([]byte{byte(v)}, payload...)
}

func TestDecodeStored(t *testing.T) {
	acct := stored(t, testAccount)
	signer := stored(t, testSigner)
	if s, err := decodeStored(acct, accountColumn); err != nil || s != testAccount {
		t.Fatalf("account: %q %v", s, err)
	}
	if s, err := decodeStored(signer, signerColumn); err != nil || s != testSigner {
		t.Fatalf("signer: %q %v", s, err)
	}
	// 79ea2acd's AddressBytea keeps the version byte and the first 32 payload
	// bytes of anything it is given (#716)
	if _, err := decodeStored(signer[:33], signerColumn); err == nil {
		t.Fatal("a signed payload cut to 32 bytes decoded")
	}
	muxed := append([]byte{byte(strkey.VersionByteMuxedAccount)}, acct[1:]...)
	if _, err := decodeStored(muxed, accountColumn); err == nil {
		t.Fatal("a muxed account cut to 32 bytes decoded")
	}
	if _, err := decodeStored(signer, accountColumn); err == nil {
		t.Fatal("a signed payload in an account column decoded as an account")
	}
	if _, err := decodeStored([]byte{0xff, 1, 2}, accountColumn); err == nil {
		t.Fatal("an unknown version byte decoded")
	}
}

func scRow(cat, reason string, op, id int64, cols map[string]any) row {
	r := row{"state_change_category": cat, "state_change_reason": reason, "operation_id": op, "state_change_id": id, "to_id": int64(65) << 32}
	maps.Copy(r, cols)
	return r
}

func contractOf(t *testing.T, a xdr.Asset) []byte {
	t.Helper()
	id, err := a.ContractID(testPassphrase)
	if err != nil {
		t.Fatal(err)
	}
	return append([]byte{byte(strkey.VersionByteContract)}, id[:]...)
}

func testMapper(t *testing.T) (*mapper, []byte, []byte) {
	t.Helper()
	native := contractOf(t, xdr.MustNewNativeAsset())
	usd := contractOf(t, xdr.MustNewCreditAsset("USD", testIssuer))
	var nid, uid [32]byte
	copy(nid[:], native[1:])
	copy(uid[:], usd[1:])
	return &mapper{assets: assetIndex{byContract: map[[32]byte]string{nid: "native", uid: "USD:" + testIssuer}}}, native, usd
}

func changesJSON(t *testing.T, m txMapping) []map[string]any {
	t.Helper()
	var out []map[string]any
	for _, c := range m.changes {
		b, err := json.Marshal(c)
		if err != nil {
			t.Fatal(err)
		}
		var v map[string]any
		if err := json.Unmarshal(b, &v); err != nil {
			t.Fatal(err)
		}
		out = append(out, v)
	}
	return out
}

func TestMapSignerAndFee(t *testing.T) {
	m, native, _ := testMapper(t)
	acct := stored(t, testAccount)
	op := int64(65)<<32 | 1
	got := m.mapTransaction([]row{
		scRow(catSigner, reasonAdd, op, 3, map[string]any{"account_id": acct, "signer_account_id": stored(t, testSigner), "signer_weight_new": int16(1)}),
		scRow(catBalance, reasonDebit, 0, 1, map[string]any{"account_id": acct, "token_id": native, "amount": "100"}),
	})
	if len(got.unmapped) != 0 {
		t.Fatalf("unmapped: %+v", got.unmapped)
	}
	cs := changesJSON(t, got)
	if len(cs) != 2 {
		t.Fatalf("changes: %v", cs)
	}
	fee, signer := cs[0], cs[1]
	if fee["kind"] != "token-event" || fee["event"] != "fee" || fee["from"] != testAccount || fee["amount"] != "100" || fee["scope"] != "fee-processing" {
		t.Fatalf("fee: %v", fee)
	}
	if signer["kind"] != "signer-added" || signer["signer"] != testSigner || signer["weight"] != float64(1) || signer["op_index"] != float64(0) {
		t.Fatalf("signer: %v", signer)
	}
}

func TestMapUndecodableSigner(t *testing.T) {
	m, _, _ := testMapper(t)
	got := m.mapTransaction([]row{
		scRow(catSigner, reasonAdd, int64(65)<<32|1, 3, map[string]any{"account_id": stored(t, testAccount), "signer_account_id": stored(t, testSigner)[:33], "signer_weight_new": int16(1)}),
	})
	if len(got.changes) != 1 {
		t.Fatalf("changes: %+v", got.changes)
	}
	c := got.changes[0]
	if c.Fields["signer"] != nil {
		t.Fatalf("signer field is %v, want null", c.Fields["signer"])
	}
	raw := c.Raw["signer"]
	if raw.Error == "" || raw.Hex != hex.EncodeToString(stored(t, testSigner)[:33]) {
		t.Fatalf("raw: %+v", raw)
	}
}

func TestMapTransferMintAndBurnPairs(t *testing.T) {
	m, _, usd := testMapper(t)
	from := stored(t, testAccount)
	to := stored(t, "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5")
	issuer := stored(t, testIssuer)
	op := int64(65)<<32 | 1
	got := m.mapTransaction([]row{
		scRow(catBalance, reasonDebit, op, 1, map[string]any{"account_id": from, "token_id": usd, "amount": "5"}),
		scRow(catBalance, reasonCredit, op, 2, map[string]any{"account_id": to, "token_id": usd, "amount": "5", "to_muxed_id": "7"}),
		scRow(catBalance, reasonMint, op, 3, map[string]any{"account_id": issuer, "token_id": usd, "amount": "9"}),
		scRow(catBalance, reasonCredit, op, 4, map[string]any{"account_id": to, "token_id": usd, "amount": "9"}),
		scRow(catBalance, reasonBurn, op, 5, map[string]any{"account_id": issuer, "token_id": usd, "amount": "2"}),
		scRow(catBalance, reasonDebit, op, 6, map[string]any{"account_id": from, "token_id": usd, "amount": "2"}),
	})
	cs := changesJSON(t, got)
	if len(cs) != 3 || len(got.unmapped) != 0 {
		t.Fatalf("changes %v unmapped %v", cs, got.unmapped)
	}
	mux, _ := cs[0]["to_muxed_id"].(map[string]any)
	if cs[0]["event"] != "transfer" || cs[0]["from"] != testAccount || cs[0]["to"] == nil || mux["type"] != "u64" || mux["value"] != "7" {
		t.Fatalf("transfer: %v", cs[0])
	}
	if cs[1]["event"] != "mint" || cs[1]["to"] != "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5" || cs[1]["amount"] != "9" {
		t.Fatalf("mint: %v", cs[1])
	}
	if cs[2]["event"] != "burn" || cs[2]["from"] != testAccount {
		t.Fatalf("burn: %v", cs[2])
	}
}

// a liquidity-pool leg is one DEBIT row of its own; the next event's DEBIT and
// CREDIT, with the same token and amount, pair with each other and not with it
func TestBalancePairsStayWithinOneEvent(t *testing.T) {
	m, _, usd := testMapper(t)
	from := stored(t, testAccount)
	other := stored(t, testIssuer)
	to := stored(t, "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5")
	op := int64(65)<<32 | 1
	got := m.mapTransaction([]row{
		scRow(catBalance, reasonDebit, op, 1, map[string]any{"account_id": from, "token_id": usd, "amount": "5"}),
		scRow(catBalance, reasonDebit, op, 2, map[string]any{"account_id": other, "token_id": usd, "amount": "5"}),
		scRow(catBalance, reasonCredit, op, 3, map[string]any{"account_id": to, "token_id": usd, "amount": "5"}),
		// a gap in the ids: not one event, so no pair
		scRow(catBalance, reasonDebit, op, 5, map[string]any{"account_id": from, "token_id": usd, "amount": "1"}),
		scRow(catBalance, reasonCredit, op, 7, map[string]any{"account_id": to, "token_id": usd, "amount": "1"}),
	})
	cs := changesJSON(t, got)
	if len(cs) != 4 || len(got.unmapped) != 0 {
		t.Fatalf("changes %v unmapped %v", cs, got.unmapped)
	}
	if cs[0]["event"] != "transfer" || cs[0]["from"] != testAccount || cs[0]["to"] != nil {
		t.Fatalf("one-sided pool leg: %v", cs[0])
	}
	if cs[1]["from"] != testIssuer || cs[1]["to"] != "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5" {
		t.Fatalf("pair of the second event: %v", cs[1])
	}
	if cs[2]["to"] != nil || cs[3]["from"] != nil {
		t.Fatalf("rows with non-consecutive ids paired: %v %v", cs[2], cs[3])
	}
}

// a BALANCE_AUTHORIZATION holder that is neither G nor C is reported as
// unmapped, not sent down the SAC set_authorized path
func TestBalanceAuthorizationUnknownHolder(t *testing.T) {
	m, _, usd := testMapper(t)
	op := int64(65)<<32 | 1
	muxed := append([]byte{byte(strkey.VersionByteMuxedAccount)}, stored(t, testAccount)[1:]...)
	contract := append([]byte{byte(strkey.VersionByteContract)}, make([]byte, 32)...)
	got := m.mapTransaction([]row{
		scRow(catBalanceAuth, reasonSet, op, 1, map[string]any{"account_id": muxed, "token_id": usd}),
		scRow(catBalanceAuth, reasonSet, op, 2, map[string]any{"account_id": contract, "token_id": usd}),
	})
	cs := changesJSON(t, got)
	if len(got.unmapped) != 1 || !strings.Contains(got.unmapped[0].Note, "neither an account (G) nor a contract (C)") {
		t.Fatalf("unmapped %+v", got.unmapped)
	}
	if len(cs) != 1 || cs[0]["event"] != "set_authorized" {
		t.Fatalf("changes %v", cs)
	}
}

func TestMapTrustlineMergesInitialFlags(t *testing.T) {
	m, _, usd := testMapper(t)
	acct := stored(t, testAccount)
	op := int64(65)<<32 | 1
	got := m.mapTransaction([]row{
		scRow(catTrustline, reasonAdd, op, 1, map[string]any{"account_id": acct, "token_id": usd, "trustline_limit_new": "922337203685.4775807"}),
		scRow(catBalanceAuth, reasonSet, op, 2, map[string]any{"account_id": acct, "token_id": usd, "flags": int16(wbFlagAuthorized)}),
	})
	cs := changesJSON(t, got)
	if len(cs) != 1 || len(got.unmapped) != 0 {
		t.Fatalf("changes %v unmapped %v", cs, got.unmapped)
	}
	c := cs[0]
	if c["kind"] != "trustline-created" || c["asset"] != "USD:"+testIssuer || c["limit"] != "9223372036854775807" || c["flags"] != float64(1) {
		t.Fatalf("trustline: %v", c)
	}
}

func TestUnknownTokenAndBlendRowsAreUnmapped(t *testing.T) {
	m, _, _ := testMapper(t)
	acct := stored(t, testAccount)
	other := contractOf(t, xdr.MustNewCreditAsset("EUR", testIssuer))
	op := int64(65)<<32 | 1
	got := m.mapTransaction([]row{
		scRow(catTrustline, reasonAdd, op, 1, map[string]any{"account_id": acct, "token_id": other, "trustline_limit_new": "0.0000001"}),
		scRow("BLEND_SUPPLY", reasonCredit, op, 2, map[string]any{"account_id": acct}),
	})
	if len(got.changes) != 0 || len(got.unmapped) != 2 {
		t.Fatalf("changes %+v unmapped %+v", got.changes, got.unmapped)
	}
	if got.unmapped[0].Row["account_id"] != hex.EncodeToString(acct) {
		t.Fatalf("unmapped row bytes are not hex: %v", got.unmapped[0].Row)
	}
}

func TestLastError(t *testing.T) {
	logs := strings.Join([]string{
		`time="x" level=info msg="Ingested ledger 3 in 0.07s" pid=1`,
		`time="x" level=warning msg="blend: price snapshot pass failed: instance entry not found" pid=1`,
		`time="x" level=warning msg="Error ingesting data for ledger 4 (attempt 1/5): COPY from stdin failed" pid=1`,
		`time="x" level=info msg="Shutting down servers..." pid=1`,
	}, "\n")
	if got := lastError(logs); !strings.Contains(got, "Error ingesting data for ledger 4") {
		t.Fatalf("last error %q", got)
	}
	if got := lastError(logs + "\npanic: executing root command: running ingest"); !strings.HasPrefix(got, "panic: ") {
		t.Fatalf("last error %q", got)
	}
	if got := lastError(`time="x" level=warning msg="blend: price snapshot pass failed" pid=1`); got != "" {
		t.Fatalf("a non-ingestion warning became the error: %q", got)
	}
}

// the image references must stay the ones pins.toml verifies
func TestImagesMatchPins(t *testing.T) {
	pins, err := os.ReadFile("../../pins.toml")
	if err != nil {
		t.Fatal(err)
	}
	for _, img := range []string{TimescaleImage, SeaweedImage} {
		repoTag, digest, ok := strings.Cut(img, "@")
		if !ok {
			t.Fatalf("%s has no digest", img)
		}
		repo, tag, _ := strings.Cut(repoTag, ":")
		for _, want := range []string{`repository = "` + repo + `"`, `tag = "` + tag + `"`, `digest = "` + digest + `"`} {
			if !strings.Contains(string(pins), want) {
				t.Errorf("pins.toml has no %s for %s", want, img)
			}
		}
	}
}

func TestStroops(t *testing.T) {
	for in, want := range map[string]string{"922337203685.4775807": "9223372036854775807", "1": "10000000", "0.5": "5000000", "12.0000001": "120000001"} {
		if got, ok := stroops(in); !ok || got != want {
			t.Errorf("%s: %q %v", in, got, ok)
		}
	}
	for _, bad := range []string{"", "1.12345678", "-1", "1e5", ".5"} {
		if _, ok := stroops(bad); ok {
			t.Errorf("%q converted", bad)
		}
	}
}

func TestPoolShareAsset(t *testing.T) {
	m, _, _ := testMapper(t)
	got := m.mapTransaction([]row{
		scRow(catTrustline, reasonRemove, int64(65)<<32|1, 1, map[string]any{"account_id": stored(t, testAccount), "liquidity_pool_id": "79d95b12eb2e5c9bc625aa5e059360ec35a1be8a7a4a7fdb90a04c8e5f814ba4"}),
	})
	if len(got.changes) != 1 || got.changes[0].Fields["asset"] != "pool-share:LB45SWYS5MXFZG6GEWVF4BMTMDWDLIN6RJ5EU763SCQEZDS7QFF2IPOF" {
		t.Fatalf("changes %+v unmapped %+v", got.changes, got.unmapped)
	}
}

// every pinned base must be the image pins.toml verifies, by repository, tag and digest
func TestBaseImagesMatchPins(t *testing.T) {
	pins, err := os.ReadFile("../../pins.toml")
	if err != nil {
		t.Fatal(err)
	}
	for from, pinned := range BaseImages {
		repo, tag, ok := strings.Cut(from, ":")
		if !ok {
			t.Fatalf("%s has no tag", from)
		}
		if !strings.Contains(repo, "/") {
			repo = "library/" + repo
		}
		ref, digest, ok := strings.Cut(pinned, "@")
		if !ok || ref != "docker.io/"+repo {
			t.Fatalf("%s is pinned as %s, want docker.io/%s@<digest>", from, pinned, repo)
		}
		entry := `repository = "` + repo + `"` + "\n" + `tag = "` + tag + `"` + "\n" + `digest = "` + digest + `"`
		if !strings.Contains(string(pins), entry) {
			t.Errorf("pins.toml has no entry with\n%s", entry)
		}
	}
}

func TestDockerfileBases(t *testing.T) {
	df := []byte(`# comment FROM nothing
FROM golang:1.25.9-bookworm AS api-build
RUN echo from here is not a FROM
from --platform=linux/amd64 debian:bookworm-slim AS runtime
FROM api-build AS again
COPY --from=api-build /x /y
FROM golang:1.25.9-bookworm
FROM scratch
`)
	got, err := dockerfileBases(df)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(got, " ") != "golang:1.25.9-bookworm debian:bookworm-slim" {
		t.Fatalf("bases %q", got)
	}
	flags, set, err := pinnedContexts(got)
	if err != nil {
		t.Fatal(err)
	}
	if len(flags) != 4 || flags[1] != "golang:1.25.9-bookworm=docker-image://"+BaseImages["golang:1.25.9-bookworm"] || len(set) != 64 {
		t.Fatalf("flags %q set %q", flags, set)
	}
	if _, _, err := pinnedContexts([]string{"alpine:3"}); err == nil {
		t.Fatal("an unpinned base was accepted")
	}
	if _, err := dockerfileBases([]byte("FROM golang:${GO}\n")); err == nil {
		t.Fatal("a FROM with a build argument was accepted")
	}
}
