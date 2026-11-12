package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"reflect"
	"testing"

	"github.com/stellar/go-stellar-sdk/xdr"
)

// the examples are the ones in SEP-51 2.0.1 itself, as base64 xdr and the json it gives
func TestSEP51Examples(t *testing.T) {
	cases := []struct {
		name  string
		value any
		xdr   string
		json  string
	}{
		{"asset native", new(xdr.Asset), "AAAAAA==", `"native"`},
		{"asset alphanum4", new(xdr.Asset), "AAAAAUFCQ0QAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
			`{"credit_alphanum4":{"asset_code":"ABCD","issuer":"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"}}`},
		{"scaddress muxed", new(xdr.ScAddress), "AAAAAgAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
			`"MAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFNZG"`},
		{"asset code 4", new(xdr.AssetCode4), "QUJDAA==", `"ABC"`},
		{"asset code 12 of five", new(xdr.AssetCode12), "QUJDREUAAAAAAAAA", `"ABCDE"`},
		{"asset code 12 of three", new(xdr.AssetCode12), "QUJDAAAAAAAAAAAA", `"ABC\\0\\0"`},
		{"string", new(xdr.ScString), "AAAAC2hlbGxvw3dvcmxkAA==", `"hello\\xc3world"`},
		{"hyper", new(xdr.Int64), "f/////////8=", `"9223372036854775807"`},
		{"unsigned hyper", new(xdr.Uint64), "//////////8=", `"18446744073709551615"`},
		{"int", new(xdr.Int32), "f////w==", `2147483647`},
		{"bool", new(bool), "AAAAAQ==", `true`},
		{"int-switched union", new(xdr.SorobanTransactionMetaExt), "AAAAAA==", `"v0"`},
		{"transaction envelope",
			new(xdr.TransactionEnvelope),
			"AAAAAgAAAADmmSZkwY3163TMouB2TY8MljqXw2IxVYTGyvDrR6YtAAAqmmQAABpuAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAQAAAAEAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAAAAAABAAAABgAAAAHXkotywnA8z+r365/0701QSlWouXn8m0UOoshCtNHOYQAAABQAAAABAAI9fQAAAAAAAAD4AAAAAAAqmgAAAAABR6YtAAAAAEArDtxbqUI+CsdkRmV0lFhVt0wyB7fyrmmkM6Fr35wpPcK8WKcXeKTl4BQ+akE14MZtpaea9LMdhXopaW3pJA0E",
			`{"tx":{"tx":{"source_account":"GDTJSJTEYGG7L23UZSROA5SNR4GJMOUXYNRDCVMEY3FPB22HUYWQBZIA","fee":2792036,"seq_num":"29059748724737","cond":"none","memo":"none",
			"operations":[{"source_account":null,"body":{"invoke_host_function":{"host_function":{"create_contract":{"contract_id_preimage":{"asset":"native"},"executable":"stellar_asset"}},"auth":[]}}}],
			"ext":{"v1":{"ext":"v0","resources":{"footprint":{"read_only":[],"read_write":[{"contract_data":{"contract":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","key":"ledger_key_contract_instance","durability":"persistent"}}]},
			"instructions":146813,"disk_read_bytes":0,"write_bytes":248},"resource_fee":"2791936"}}},
			"signatures":[{"hint":"47a62d00","signature":"2b0edc5ba9423e0ac764466574945855b74c3207b7f2ae69a433a16bdf9c293dc2bc58a71778a4e5e0143e6a4135e0c66da5a79af4b31d857a29696de9240d04"}]}}`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			data, err := base64.StdEncoding.DecodeString(c.xdr)
			if err != nil {
				t.Fatal(err)
			}
			if err := xdr.SafeUnmarshal(data, c.value); err != nil {
				t.Fatalf("decode: %v", err)
			}
			got, err := projectSEP51(reflect.ValueOf(c.value).Elem())
			if err != nil {
				t.Fatalf("project: %v", err)
			}
			assertJSON(t, got, c.json)
		})
	}
}

func assertJSON(t *testing.T, got any, want string) {
	t.Helper()
	gotText, err := json.Marshal(got)
	if err != nil {
		t.Fatal(err)
	}
	var g, w any
	if err := json.Unmarshal(gotText, &g); err != nil {
		t.Fatal(err)
	}
	dec := json.NewDecoder(bytes.NewReader([]byte(want)))
	if err := dec.Decode(&w); err != nil {
		t.Fatalf("want is not json: %v", err)
	}
	if !reflect.DeepEqual(g, w) {
		t.Errorf("got  %s\nwant %s", gotText, want)
	}
}

func TestEnumNames(t *testing.T) {
	cases := []struct {
		typ   reflect.Type
		value int32
		want  string
	}{
		// shared prefix SCV_ dropped
		{reflect.TypeFor[xdr.ScValType](), int32(xdr.ScValTypeScvU32), "u32"},
		{reflect.TypeFor[xdr.ScValType](), int32(xdr.ScValTypeScvLedgerKeyContractInstance), "ledger_key_contract_instance"},
		// txSUCCESS and txFAILED share "tx" but no underscore, so nothing is dropped
		{reflect.TypeFor[xdr.TransactionResultCode](), int32(xdr.TransactionResultCodeTxSuccess), "tx_success"},
		{reflect.TypeFor[xdr.OperationResultCode](), int32(xdr.OperationResultCodeOpInner), "op_inner"},
		{reflect.TypeFor[xdr.AssetType](), int32(xdr.AssetTypeAssetTypeCreditAlphanum12), "credit_alphanum12"},
		// a digit after an underscore joins the word before it, and a leading digit gets a B
		{reflect.TypeFor[xdr.BinaryFuseFilterType](), int32(xdr.BinaryFuseFilterTypeBinaryFuseFilter16Bit), "b16_bit"},
		// one identifier keeps its whole name
		{reflect.TypeFor[xdr.LiquidityPoolType](), int32(xdr.LiquidityPoolTypeLiquidityPoolConstantProduct), "liquidity_pool_constant_product"},
	}
	for _, c := range cases {
		got, err := enumName(c.typ, c.value)
		if err != nil {
			t.Errorf("%s %d: %v", c.typ, c.value, err)
			continue
		}
		if got != c.want {
			t.Errorf("%s %d: %q, want %q", c.typ, c.value, got, c.want)
		}
	}
}

func TestUnderscore(t *testing.T) {
	for in, want := range map[string]string{
		"LiveUntilLedgerSeq": "live_until_ledger_seq",
		"ContractId":         "contract_id",
		"ContractID":         "contract_id",
		"AlphaNum4":          "alpha_num4",
		"Ed25519":            "ed25519",
		"V1":                 "v1",
		"HashX":              "hash_x",
	} {
		if got := underscore(in); got != want {
			t.Errorf("underscore(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestWideIntegers(t *testing.T) {
	cases := []struct {
		value any
		want  string
	}{
		{xdr.Int128Parts{Hi: -1, Lo: 0xffffffffffffffff}, "-1"},
		{xdr.Int128Parts{Hi: -1, Lo: 0}, "-18446744073709551616"},
		{xdr.UInt128Parts{Hi: 1, Lo: 0}, "18446744073709551616"},
		{xdr.Int256Parts{HiHi: -0x8000000000000000}, "-57896044618658097711785492504343953926634992332820282019728792003956564819968"},
		{xdr.UInt256Parts{LoLo: 5}, "5"},
	}
	for _, c := range cases {
		got, err := projectSEP51(reflect.ValueOf(c.value))
		if err != nil {
			t.Fatal(err)
		}
		if got != c.want {
			t.Errorf("%#v: %v, want %s", c.value, got, c.want)
		}
	}
}
