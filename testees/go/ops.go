package main

import (
	"bytes"
	"encoding"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"reflect"
	"runtime/debug"

	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"
)

const (
	sdkModule = "github.com/stellar/go-stellar-sdk"
	xdrModule = "github.com/stellar/go-xdr"
)

// ops lists what this build implements, as hello reports it. The other protocol ops have no
// go-stellar-sdk api; dispatch answers them with the reason.
var ops = []string{
	"decode_encode",
	"decode_json",
	"strkey_decode",
	"strkey_encode",
	"asset_order",
	"pool_id",
	"tx_hash",
	"balance_id",
	"contract_id",
	"id_parse",
}

var unsupportedOps = map[string]string{
	"encode_json": "go-stellar-sdk has no SEP-51 XDR-JSON reader: package xdr reads xdr bytes, " +
		"and its only json codec is a horizon-style one for xdr.ClaimPredicate",
	"map_sort": "go-stellar-sdk has no ScMap constructor that sorts keys: xdr.ScMap is a plain " +
		"slice of ScMapEntry and the sdk keeps entries in the order given",
	"map_validate": "go-stellar-sdk has no ScMap validation: xdr.ScMap decodes any key order and " +
		"no sdk function checks it",
	"to_native": "go-stellar-sdk has no native projection of an ScVal: callers read the typed " +
		"xdr.ScVal arms, and ScVal.String is a display string, not a native value",
}

func dispatch(req request) (map[string]any, error) {
	switch req.op {
	case "hello":
		return hello()
	case "decode_encode":
		return decodeEncode(req)
	case "decode_json":
		return decodeJSON(req)
	case "strkey_decode":
		return strkeyDecode(req)
	case "strkey_encode":
		return strkeyEncode(req)
	case "asset_order":
		return assetOrder(req)
	case "pool_id":
		return poolID(req)
	case "tx_hash":
		return txHash(req)
	case "balance_id":
		return balanceID(req)
	case "contract_id":
		return contractID(req)
	case "id_parse":
		return idParse(req)
	case "coverage_snapshot":
		return coverageSnapshot(req)
	}
	if reason, ok := unsupportedOps[req.op]; ok {
		return nil, unsupported(reason)
	}
	return nil, badRequest("unknown op %s", req.op)
}

// helloOps lists coverage_snapshot only for a build that can answer it
func helloOps() []string {
	if instrumented() {
		return append(append([]string{}, ops...), "coverage_snapshot")
	}
	return ops
}

func hello() (map[string]any, error) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return nil, internal("the binary has no build info to read the sdk version from")
	}
	sdkVersion, sdkDir, err := depVersion(info, sdkModule, sdkVersionOverride)
	if err != nil {
		return nil, err
	}
	xdrVersion, _, err := depVersion(info, xdrModule, "")
	if err != nil {
		return nil, err
	}
	components := map[string]string{xdrModule: xdrVersion}
	if sdkDir != "" {
		components[sdkModule+" directory"] = sdkDir
	}
	return map[string]any{
		"protocol":    protocolVersion,
		"sdk":         sdkModule,
		"sdk_version": sdkVersion,
		"components":  components,
		"ops":         helloOps(),
		// DecodeOptions carries both: MaxDepth is the depth, MaxInputLen the length
		"decode_limits": "honoured", //nolint:misspell // the protocol spells it this way
	}, nil
}

// sdkVersionOverride is the sdk version a build against a directory replace of the sdk
// reports, set at link time with -ldflags "-X main.sdkVersionOverride=<version>". That is the
// build an sdk's own pull request gets in its CI (docs/sdk-ci.md): the module system gives a
// directory no version, so the version comes from the caller's versions file, which the runner
// checks hello against. Empty in every other build, and ignored unless the sdk is replaced by
// a directory.
var sdkVersionOverride string

// depVersion returns the version path is built at, and the directory it was replaced by when
// that replace is a directory. A directory replace has no version of its own: it reports
// override, and is refused when override is empty.
func depVersion(info *debug.BuildInfo, path, override string) (version, dir string, err error) {
	for _, d := range info.Deps {
		if d.Path != path {
			continue
		}
		if d.Replace != nil {
			// a directory replace has no module version; the build info shows it as (devel)
			if d.Replace.Version != "" && d.Replace.Version != "(devel)" {
				return d.Replace.Version, "", nil
			}
			if override == "" {
				return "", d.Replace.Path, internal("%s is replaced by %s, which has no version; "+
					"build with -ldflags \"-X main.sdkVersionOverride=<version>\"", path, d.Replace.Path)
			}
			return override, d.Replace.Path, nil
		}
		return d.Version, "", nil
	}
	return "", "", internal("%s is not in the build info", path)
}

// decode reads the request's bytes as one value of the named type through the sdk's decoder,
// with the runner's limits, and refuses trailing bytes as SafeUnmarshal does.
func decode(req request) (any, error) {
	name, err := req.str("type")
	if err != nil {
		return nil, err
	}
	data, err := req.bytes("xdr")
	if err != nil {
		return nil, err
	}
	depth, err := req.uint32("depth_limit")
	if err != nil {
		return nil, err
	}
	maxLen, err := req.nonNegativeInt("len_limit")
	if err != nil {
		return nil, err
	}
	newValue, ok := xdrTypes[name]
	if !ok {
		return nil, unsupported("the testee's type table, generated from the corpus manifest " +
			"against go-stellar-sdk's xdr package, has no " + name)
	}
	value := newValue()
	var read int
	opts := xdr.DecodeOptions{MaxDepth: uint(depth), MaxInputLen: maxLen}
	err = sdkCall("xdr.UnmarshalWithOptions", func() error {
		var e error
		read, e = xdr.UnmarshalWithOptions(bytes.NewReader(data), value, opts)
		return e
	})
	if err != nil {
		return nil, err
	}
	if read != len(data) {
		return nil, &opError{class: classRejected, message: fmt.Sprintf("input not fully consumed: decoded %d of %d bytes", read, len(data))}
	}
	return value, nil
}

func decodeEncode(req request) (map[string]any, error) {
	value, err := decode(req)
	if err != nil {
		return nil, err
	}
	m, ok := value.(encoding.BinaryMarshaler)
	if !ok {
		return nil, internal("%T has no MarshalBinary", value)
	}
	var out []byte
	err = sdkCall("MarshalBinary", func() error {
		var e error
		out, e = m.MarshalBinary()
		return e
	})
	if err != nil {
		return nil, err
	}
	return map[string]any{"xdr": base64.StdEncoding.EncodeToString(out)}, nil
}

func decodeJSON(req request) (map[string]any, error) {
	value, err := decode(req)
	if err != nil {
		return nil, err
	}
	rendered, err := projectSEP51(reflect.ValueOf(value).Elem())
	if err != nil {
		return nil, err
	}
	return map[string]any{"json": rendered}, nil
}

// strkey kinds of the protocol and their sep-23 version bytes. W has no named constant before
// v0.8.0, so it is given as the byte sep-23 assigns; strkey.Encode and DecodeAny check it
// against the version bytes the linked sdk knows, which is the behavior under test.
var strkeyKinds = []struct {
	kind    string
	version strkey.VersionByte
}{
	{"account", strkey.VersionByteAccountID},
	{"muxed_account", strkey.VersionByteMuxedAccount},
	{"seed", strkey.VersionByteSeed},
	{"pre_auth_tx", strkey.VersionByteHashTx},
	{"hash_x", strkey.VersionByteHashX},
	{"signed_payload", strkey.VersionByteSignedPayload},
	{"contract", strkey.VersionByteContract},
	{"liquidity_pool", strkey.VersionByteLiquidityPool},
	{"claimable_balance", strkey.VersionByteClaimableBalance},
	{"muxed_contract", versionByteMuxedContract},
}

const versionByteMuxedContract = strkey.VersionByte(22 << 3)

func strkeyDecode(req request) (map[string]any, error) {
	text, err := strkeyText(req)
	if err != nil {
		return nil, err
	}
	var version strkey.VersionByte
	var payload []byte
	err = sdkCall("strkey.DecodeAny", func() error {
		var e error
		version, payload, e = strkey.DecodeAny(text)
		return e
	})
	if err != nil {
		return nil, err
	}
	for _, k := range strkeyKinds {
		if k.version == version {
			// DecodeAny returns the raw payload, which is the protocol's layout for every kind
			return map[string]any{"kind": k.kind, "payload": base64.StdEncoding.EncodeToString(payload)}, nil
		}
	}
	return nil, internal("strkey.DecodeAny accepted version byte %d, which no protocol kind has", version)
}

// strkeyText reads text, or text_base64 for input that is not utf-8. a go string holds any
// bytes, so both reach strkey.DecodeAny unchanged.
func strkeyText(req request) (string, error) {
	_, hasText := req.fields["text"]
	_, hasBytes := req.fields["text_base64"]
	switch {
	case hasText && !hasBytes:
		return req.str("text")
	case hasBytes && !hasText:
		b, err := req.bytes("text_base64")
		return string(b), err
	default:
		return "", badRequest("strkey_decode needs exactly one of text and text_base64")
	}
}

func strkeyEncode(req request) (map[string]any, error) {
	kind, err := req.str("kind")
	if err != nil {
		return nil, err
	}
	payload, err := req.bytes("payload")
	if err != nil {
		return nil, err
	}
	for _, k := range strkeyKinds {
		if k.kind != kind {
			continue
		}
		var text string
		err := sdkCall("strkey.Encode", func() error {
			var e error
			text, e = strkey.Encode(k.version, payload)
			return e
		})
		if err != nil {
			return nil, err
		}
		return map[string]any{"text": text}, nil
	}
	return nil, badRequest("unknown strkey kind %s", kind)
}

func assetMember(req request, name string) (xdr.Asset, error) {
	var asset xdr.Asset
	data, err := req.bytes(name)
	if err != nil {
		return asset, err
	}
	err = sdkCall("xdr.SafeUnmarshal", func() error {
		return xdr.SafeUnmarshal(data, &asset)
	})
	return asset, err
}

func assetPair(req request) (a, b xdr.Asset, err error) {
	if a, err = assetMember(req, "a"); err != nil {
		return a, b, err
	}
	b, err = assetMember(req, "b")
	return a, b, err
}

// assetOrder asks xdr.Asset.LessThan both ways, since the sdk only has "less than".
func assetOrder(req request) (map[string]any, error) {
	a, b, err := assetPair(req)
	if err != nil {
		return nil, err
	}
	var less, greater bool
	err = sdkCall("xdr.Asset.LessThan", func() error {
		less = a.LessThan(b)
		greater = b.LessThan(a)
		return nil
	})
	if err != nil {
		return nil, err
	}
	order := 0
	switch {
	case less:
		order = -1
	case greater:
		order = 1
	}
	return map[string]any{"order": order}, nil
}

func poolID(req request) (map[string]any, error) {
	a, b, err := assetPair(req)
	if err != nil {
		return nil, err
	}
	fee, err := req.int32("fee")
	if err != nil {
		return nil, err
	}
	var id xdr.PoolId
	err = sdkCall("xdr.NewPoolId", func() error {
		var e error
		id, e = xdr.NewPoolId(a, b, xdr.Int32(fee))
		return e
	})
	if err != nil {
		return nil, err
	}
	return map[string]any{"pool_id": hex.EncodeToString(id[:])}, nil
}
