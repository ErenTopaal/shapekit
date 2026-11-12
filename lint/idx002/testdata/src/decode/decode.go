// Package decode holds seeded IDX002 decoding cases and their clean
// counterparts.
package decode

import (
	"bytes"
	"math"

	"github.com/stellar/go-stellar-sdk/xdr"
	xdr3 "github.com/stellar/go-xdr/xdr3"
)

// unlimited turns the decoder's depth limit off by name.
func unlimited(b []byte) (xdr.ScVal, error) {
	var v xdr.ScVal
	_, err := xdr.UnmarshalWithOptions(bytes.NewReader(b), &v, xdr.DecodeOptions{MaxDepth: xdr3.DecodeUnlimitedDepth}) // want `IDX002: xdr decoding with the depth limit switched off`

	return v, err
}

// maxUint does the same with the number written out.
func maxUint(b []byte) (xdr.ScVal, error) {
	var v xdr.ScVal
	_, err := xdr.UnmarshalWithOptions(bytes.NewReader(b), &v, xdr.DecodeOptions{MaxDepth: math.MaxUint}) // want `IDX002: xdr decoding with the depth limit switched off`

	return v, err
}

// assigned switches it off on a copy of the defaults.
func assigned(b []byte) (xdr.LedgerCloseMeta, error) {
	opts := xdr3.DefaultDecodeOptions
	opts.MaxDepth = math.MaxUint // want `IDX002: xdr decoding with the depth limit switched off`

	var m xdr.LedgerCloseMeta
	_, err := xdr.UnmarshalWithOptions(bytes.NewReader(b), &m, opts)

	return m, err
}

// decodeFrom passes an unlimited depth to the generated decoder directly.
func decodeFrom(b []byte) (xdr.ScVal, error) {
	var v xdr.ScVal
	_, err := v.DecodeFrom(xdr3.NewDecoder(bytes.NewReader(b)), math.MaxUint) // want `IDX002: xdr decoding with the depth limit switched off`

	return v, err
}

// defaults keeps the sdk's limits: depth 1500 and the input length.
func defaults(b []byte) (xdr.ScVal, error) {
	var v xdr.ScVal
	err := xdr.SafeUnmarshal(b, &v)

	return v, err
}

// tighter sets a smaller depth than the default.
func tighter(b []byte) (xdr.ScVal, error) {
	var v xdr.ScVal
	_, err := xdr.UnmarshalWithOptions(bytes.NewReader(b), &v, xdr.DecodeOptions{MaxDepth: 100, MaxInputLen: len(b)})

	return v, err
}

// binary uses UnmarshalBinary, which applies the defaults too.
func binary(b []byte) (xdr.TransactionEnvelope, error) {
	var env xdr.TransactionEnvelope
	err := env.UnmarshalBinary(b)

	return env, err
}

var _ = []any{unlimited, maxUint, assigned, decodeFrom, defaults, tighter, binary}
