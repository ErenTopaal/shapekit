package main

import (
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"math/big"
	"reflect"
	"regexp"
	"strconv"
	"strings"
	"sync"

	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"
)

// projectSEP51 renders a value the sdk decoded as SEP-51 XDR-JSON (2.0.1) by walking the
// sdk's own go value. go-stellar-xdr-json is a cgo build of rs-stellar-xdr, so using it would
// test the rust decoder twice; this walk only reads what go-stellar-sdk decoded.
//
// go-stellar-sdk's xdrgen output is regular enough for reflection: a struct is an xdr struct
// unless it has SwitchFieldName and ArmForSwitch, which make it a union whose arms are pointer
// fields; a pointer anywhere else is an xdr optional; []byte and [n]byte are opaque; string is
// an xdr string. Names come from the go field names and, for enums, from the xdr identifiers
// gxdr keeps, because SEP-51 derives every name from the .x spelling.
func projectSEP51(v reflect.Value) (any, error) {
	if v.Kind() == reflect.Pointer {
		if v.IsNil() {
			return nil, nil
		}
		return projectSEP51(v.Elem())
	}
	t := v.Type()
	if f, ok := specialTypes[t]; ok {
		return f(v)
	}
	if t.PkgPath() == xdrPkgPath && t.Name() == "MuxedContract" {
		// only v0.8.0 has the cap-84 type, so it is read by field name instead of by type
		return muxedContractStrkey(v)
	}
	if _, ok := enumIdentifiers[t]; ok {
		return enumName(t, int32(v.Int())) //nolint:gosec // enums are int32 kinds, so the value fits
	}
	if u, ok := reflect.TypeAssert[xdrUnion](v); ok {
		return projectUnion(v, u)
	}
	switch v.Kind() {
	case reflect.Bool:
		return v.Bool(), nil
	case reflect.Int32:
		return v.Int(), nil
	case reflect.Uint32:
		return v.Uint(), nil
	case reflect.Int64:
		return strconv.FormatInt(v.Int(), 10), nil
	case reflect.Uint64:
		return strconv.FormatUint(v.Uint(), 10), nil
	case reflect.String:
		return escapeString([]byte(v.String())), nil
	case reflect.Slice, reflect.Array:
		if t.Elem().Kind() == reflect.Uint8 {
			return hex.EncodeToString(byteSlice(v)), nil
		}
		items := make([]any, v.Len())
		for i := range v.Len() {
			item, err := projectSEP51(v.Index(i))
			if err != nil {
				return nil, err
			}
			items[i] = item
		}
		return items, nil
	case reflect.Struct:
		obj := make(map[string]any, v.NumField())
		for i := range v.NumField() {
			field := t.Field(i)
			if !field.IsExported() {
				return nil, internal("%s has unexported field %s, which xdrgen does not write", t, field.Name)
			}
			item, err := projectSEP51(v.Field(i))
			if err != nil {
				return nil, err
			}
			obj[underscore(field.Name)] = item
		}
		return obj, nil
	default:
		return nil, internal("no SEP-51 form for go type %s of kind %s", t, v.Kind())
	}
}

var xdrPkgPath = reflect.TypeFor[xdr.Asset]().PkgPath()

type xdrUnion interface {
	SwitchFieldName() string
	ArmForSwitch(sw int32) (string, bool)
}

// projectUnion writes a void arm as its discriminant name and any other arm as an object of
// one member.
func projectUnion(v reflect.Value, u xdrUnion) (any, error) {
	key, arm, err := unionArm(v, u)
	if err != nil {
		return nil, err
	}
	if !arm.IsValid() {
		return key, nil
	}
	inner, err := projectSEP51(arm)
	if err != nil {
		return nil, err
	}
	return map[string]any{key: inner}, nil
}

// unionArm returns the SEP-51 discriminant name and the arm's value, which is invalid for a
// void arm. An enum discriminant is named by its identifier; an integer one by the
// discriminant's own name and the number (switch (int v) case 1 is v1).
func unionArm(v reflect.Value, u xdrUnion) (string, reflect.Value, error) {
	t := v.Type()
	sw := v.FieldByName(u.SwitchFieldName())
	if !sw.IsValid() {
		return "", reflect.Value{}, internal("%s has no switch field %s", t, u.SwitchFieldName())
	}
	var disc int32
	switch sw.Kind() {
	case reflect.Int32:
		disc = int32(sw.Int()) //nolint:gosec // the kind is int32, so the value fits
	case reflect.Uint32:
		disc = int32(sw.Uint()) //nolint:gosec // xdr discriminants are 32 bits; ArmForSwitch takes them as int32
	default:
		return "", reflect.Value{}, internal("%s switches on %s", t, sw.Type())
	}
	var key string
	if _, ok := enumIdentifiers[sw.Type()]; ok {
		name, err := enumName(sw.Type(), disc)
		if err != nil {
			return "", reflect.Value{}, err
		}
		key = name
	} else {
		key = underscore(u.SwitchFieldName()) + strconv.FormatInt(sw.Int(), 10)
		if sw.Kind() == reflect.Uint32 {
			key = underscore(u.SwitchFieldName()) + strconv.FormatUint(sw.Uint(), 10)
		}
	}
	armName, ok := u.ArmForSwitch(disc)
	if !ok {
		return "", reflect.Value{}, internal("%s has no arm for discriminant %d", t, disc)
	}
	if armName == "" {
		return key, reflect.Value{}, nil
	}
	arm := v.FieldByName(armName)
	if !arm.IsValid() || arm.Kind() != reflect.Pointer {
		return "", reflect.Value{}, internal("%s arm %s is not a pointer field", t, armName)
	}
	if arm.IsNil() {
		return "", reflect.Value{}, internal("%s arm %s is not set for discriminant %d", t, armName, disc)
	}
	return key, arm.Elem(), nil
}

func byteSlice(v reflect.Value) []byte {
	b := make([]byte, v.Len())
	reflect.Copy(reflect.ValueOf(b), v)
	return b
}

// escapeString applies SEP-51's string rule: printable ascii as is, \0 \t \n \r \\ by name,
// every other byte as \xNN.
func escapeString(b []byte) string {
	var s strings.Builder
	for _, c := range b {
		switch {
		case c == 0:
			s.WriteString(`\0`)
		case c == '\t':
			s.WriteString(`\t`)
		case c == '\n':
			s.WriteString(`\n`)
		case c == '\r':
			s.WriteString(`\r`)
		case c == '\\':
			s.WriteString(`\\`)
		case c >= 0x20 && c <= 0x7e:
			s.WriteByte(c)
		default:
			fmt.Fprintf(&s, `\x%02x`, c)
		}
	}
	return s.String()
}

var (
	// rails' String#underscore, which xdrgen applies to .x names before it camel-cases them
	underscoreAcronym = regexp.MustCompile(`([A-Z\d]+)([A-Z][a-z])`)
	underscoreWord    = regexp.MustCompile(`([a-z\d])([A-Z])`)
)

func underscore(s string) string {
	s = underscoreAcronym.ReplaceAllString(s, "${1}_${2}")
	s = underscoreWord.ReplaceAllString(s, "${1}_${2}")
	return strings.ToLower(strings.ReplaceAll(s, "-", "_"))
}

var enumNameCache sync.Map // reflect.Type -> map[int32]string

// enumName gives the SEP-51 name of one enum value: the xdr identifier without the prefix all
// the enum's identifiers share (cut at an underscore, so txSUCCESS keeps its tx), in
// snake_case.
func enumName(t reflect.Type, value int32) (string, error) {
	cached, ok := enumNameCache.Load(t)
	if !ok {
		names, err := enumNames(t)
		if err != nil {
			return "", err
		}
		cached, _ = enumNameCache.LoadOrStore(t, names)
	}
	name, ok := cached.(map[int32]string)[value]
	if !ok {
		return "", internal("%s has no value %d in gxdr", t, value)
	}
	return name, nil
}

func enumNames(t reflect.Type) (map[int32]string, error) {
	identifiers := enumIdentifiers[t]
	if len(identifiers) == 0 {
		return nil, internal("gxdr has no identifiers for %s", t)
	}
	prefix := ""
	if len(identifiers) > 1 {
		first := true
		for _, id := range identifiers {
			if first {
				prefix, first = id, false
				continue
			}
			prefix = commonPrefix(prefix, id)
		}
		prefix = prefix[:strings.LastIndex(prefix, "_")+1]
	}
	out := make(map[int32]string, len(identifiers))
	for value, id := range identifiers {
		out[value] = snakeIdentifier(strings.TrimPrefix(id, prefix))
	}
	return out, nil
}

func commonPrefix(a, b string) string {
	n := min(len(a), len(b))
	for i := range n {
		if a[i] != b[i] {
			return a[:i]
		}
	}
	return a[:n]
}

// snakeIdentifier turns the rest of an xdr identifier into its SEP-51 name the way
// stellar-xdr does: rails underscore, camel case, then serde's snake_case, which puts an
// underscore only before an upper-case letter, so FILTER_8_BIT becomes filter8_bit. An
// identifier left starting with a digit gets stellar-xdr's B prefix (BinaryFuseFilterType's
// 8_BIT is b8_bit), since a rust variant cannot start with one.
func snakeIdentifier(id string) string {
	var camel strings.Builder
	for word := range strings.SplitSeq(underscore(id), "_") {
		if word == "" {
			continue
		}
		camel.WriteString(strings.ToUpper(word[:1]) + word[1:])
	}
	c := camel.String()
	if c != "" && c[0] >= '0' && c[0] <= '9' {
		c = "B" + c
	}
	var s strings.Builder
	for i, r := range c {
		if r >= 'A' && r <= 'Z' {
			if i > 0 {
				s.WriteByte('_')
			}
			s.WriteRune(r - 'A' + 'a')
			continue
		}
		s.WriteRune(r)
	}
	return s.String()
}

// specialTypes are the types SEP-51 renders as strings (strkeys, asset codes, wide integers)
// or as their arm alone. Each exists in every sdk version of the matrix. Strkeys come from the
// sdk's own address helpers, the ones an indexer calls, so an address the sdk cannot render
// shows up as the sdk refusing the value.
var specialTypes map[reflect.Type]func(reflect.Value) (any, error)

func init() {
	specialTypes = map[reflect.Type]func(reflect.Value) (any, error){
		reflect.TypeFor[xdr.PublicKey](): func(v reflect.Value) (any, error) {
			id := xdr.AccountId(v.Interface().(xdr.PublicKey))
			return address("xdr.AccountId.GetAddress", id.GetAddress)
		},
		reflect.TypeFor[xdr.AccountId](): func(v reflect.Value) (any, error) {
			id := v.Interface().(xdr.AccountId)
			return address("xdr.AccountId.GetAddress", id.GetAddress)
		},
		reflect.TypeFor[xdr.NodeId](): func(v reflect.Value) (any, error) {
			return address("xdr.NodeId.GetAddress", v.Interface().(xdr.NodeId).GetAddress)
		},
		reflect.TypeFor[xdr.MuxedAccount](): func(v reflect.Value) (any, error) {
			m := v.Interface().(xdr.MuxedAccount)
			return address("xdr.MuxedAccount.GetAddress", m.GetAddress)
		},
		// the two muxed ed25519 structs have no helper; ScAddress.String renders its muxed arm
		// through a MuxedAccount the same way
		reflect.TypeFor[xdr.MuxedAccountMed25519](): func(v reflect.Value) (any, error) {
			med := v.Interface().(xdr.MuxedAccountMed25519)
			m := xdr.MuxedAccount{Type: xdr.CryptoKeyTypeKeyTypeMuxedEd25519, Med25519: &med}
			return address("xdr.MuxedAccount.GetAddress", m.GetAddress)
		},
		reflect.TypeFor[xdr.MuxedEd25519Account](): func(v reflect.Value) (any, error) {
			a := v.Interface().(xdr.MuxedEd25519Account)
			m := xdr.MuxedAccount{
				Type:     xdr.CryptoKeyTypeKeyTypeMuxedEd25519,
				Med25519: &xdr.MuxedAccountMed25519{Id: a.Id, Ed25519: a.Ed25519},
			}
			return address("xdr.MuxedAccount.GetAddress", m.GetAddress)
		},
		reflect.TypeFor[xdr.ScAddress](): func(v reflect.Value) (any, error) {
			return address("xdr.ScAddress.String", v.Interface().(xdr.ScAddress).String)
		},
		// contract and pool ids have no helper of their own; ScAddress.String encodes them
		// with these version bytes
		reflect.TypeFor[xdr.ContractId](): func(v reflect.Value) (any, error) {
			id := v.Interface().(xdr.ContractId)
			return encodeStrkey(strkey.VersionByteContract, id[:])
		},
		reflect.TypeFor[xdr.PoolId](): func(v reflect.Value) (any, error) {
			id := v.Interface().(xdr.PoolId)
			return encodeStrkey(strkey.VersionByteLiquidityPool, id[:])
		},
		reflect.TypeFor[xdr.ClaimableBalanceId](): func(v reflect.Value) (any, error) {
			return address("xdr.ClaimableBalanceId.EncodeToStrkey", v.Interface().(xdr.ClaimableBalanceId).EncodeToStrkey)
		},
		reflect.TypeFor[xdr.SignerKey](): func(v reflect.Value) (any, error) {
			k := v.Interface().(xdr.SignerKey)
			return address("xdr.SignerKey.GetAddress", k.GetAddress)
		},
		reflect.TypeFor[xdr.SignerKeyEd25519SignedPayload](): func(v reflect.Value) (any, error) {
			p := v.Interface().(xdr.SignerKeyEd25519SignedPayload)
			k := xdr.SignerKey{Type: xdr.SignerKeyTypeSignerKeyTypeEd25519SignedPayload, Ed25519SignedPayload: &p}
			return address("xdr.SignerKey.GetAddress", k.GetAddress)
		},
		reflect.TypeFor[xdr.AssetCode](): func(v reflect.Value) (any, error) {
			// both arms are strings themselves, so the union adds no wrapper
			u := v.Interface().(xdr.AssetCode)
			_, arm, err := unionArm(v, u)
			if err != nil {
				return nil, err
			}
			if !arm.IsValid() {
				return nil, internal("AssetCode has a void arm")
			}
			return projectSEP51(arm)
		},
		reflect.TypeFor[xdr.AssetCode4](): func(v reflect.Value) (any, error) { return assetCode(byteSlice(v), 0), nil },
		reflect.TypeFor[xdr.AssetCode12](): func(v reflect.Value) (any, error) {
			return assetCode(byteSlice(v), 5), nil
		},
		reflect.TypeFor[xdr.Int128Parts](): func(v reflect.Value) (any, error) {
			p := v.Interface().(xdr.Int128Parts)
			return signedParts(int64(p.Hi), uint64(p.Lo)), nil
		},
		reflect.TypeFor[xdr.UInt128Parts](): func(v reflect.Value) (any, error) {
			p := v.Interface().(xdr.UInt128Parts)
			return unsignedParts(uint64(p.Hi), uint64(p.Lo)), nil
		},
		reflect.TypeFor[xdr.Int256Parts](): func(v reflect.Value) (any, error) {
			p := v.Interface().(xdr.Int256Parts)
			return signedParts(int64(p.HiHi), uint64(p.HiLo), uint64(p.LoHi), uint64(p.LoLo)), nil
		},
		reflect.TypeFor[xdr.UInt256Parts](): func(v reflect.Value) (any, error) {
			p := v.Interface().(xdr.UInt256Parts)
			return unsignedParts(uint64(p.HiHi), uint64(p.HiLo), uint64(p.LoHi), uint64(p.LoLo)), nil
		},
	}
}

// address runs one of the sdk's strkey renderers.
func address(name string, render func() (string, error)) (any, error) {
	var text string
	err := sdkCall(name, func() error {
		var e error
		text, e = render()
		return e
	})
	if err != nil {
		return nil, err
	}
	return text, nil
}

func encodeStrkey(version strkey.VersionByte, payload []byte) (any, error) {
	return address("strkey.Encode", func() (string, error) { return strkey.Encode(version, payload) })
}

// a MuxedContract outside an ScAddress (only v0.8.0 has the type) is written the way
// ScAddress.String writes its muxed contract arm
func muxedContractStrkey(v reflect.Value) (any, error) {
	contract := v.FieldByName("ContractId")
	id := v.FieldByName("Id")
	if !contract.IsValid() || contract.Kind() != reflect.Array || !id.IsValid() || id.Kind() != reflect.Uint64 {
		return nil, internal("%s does not have the ContractId and Id fields of cap-84", v.Type())
	}
	raw := byteSlice(contract)
	raw = binary.BigEndian.AppendUint64(raw, id.Uint())
	return encodeStrkey(versionByteMuxedContract, raw)
}

// assetCode drops trailing zero bytes, keeping at least minLen bytes so a 12-byte code never
// reads as a 4-byte one.
func assetCode(code []byte, minLen int) string {
	end := len(code)
	for end > minLen && code[end-1] == 0 {
		end--
	}
	return escapeString(code[:end])
}

func signedParts(hi int64, lo ...uint64) string {
	n := big.NewInt(hi)
	return appendParts(n, lo).String()
}

func unsignedParts(hi uint64, lo ...uint64) string {
	n := new(big.Int).SetUint64(hi)
	return appendParts(n, lo).String()
}

func appendParts(n *big.Int, lo []uint64) *big.Int {
	for _, part := range lo {
		n.Lsh(n, 64)
		n.Or(n, new(big.Int).SetUint64(part))
	}
	return n
}
