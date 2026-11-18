// Package alloc holds seeded IDX002 allocation cases and their clean
// counterparts.
package alloc

import (
	"errors"

	"github.com/stellar/go-stellar-sdk/xdr"
)

const maxItems = 1024

var errTooBig = errors.New("declared size too large")

// fromU32 trusts a count read out of a contract value.
func fromU32(v xdr.ScVal) []byte {
	n := v.MustU32()

	return make([]byte, n) // want `IDX002: make sized by n, an integer decoded from xdr \(v.MustU32\(\)\), with no bound check`
}

// fromU64 converts first, which does not change where the number came from.
func fromU64(v xdr.ScVal) []string {
	size := int(v.MustU64())

	return make([]string, 0, size) // want `IDX002: make sized by size`
}

// direct uses the decoded field in the make call itself.
func direct(e xdr.SorobanAddressCredentials) []int64 {
	return make([]int64, e.SignatureExpirationLedger) // want `IDX002: make sized by e.SignatureExpirationLedger`
}

// checked compares the count before it allocates.
func checked(v xdr.ScVal) ([]byte, error) {
	n := v.MustU32()
	if n > maxItems {
		return nil, errTooBig
	}

	return make([]byte, n), nil
}

// clamped bounds the size with min.
func clamped(v xdr.ScVal) []byte {
	return make([]byte, min(int(v.MustU32()), maxItems))
}

// fromLen sizes by the length of something already decoded, which is
// bounded by the input that was read.
func fromLen(vec xdr.ScVec) []xdr.ScVal {
	return make([]xdr.ScVal, 0, len(vec))
}

// constant sizes never come from the ledger.
func constant() map[string]int {
	return make(map[string]int, maxItems)
}

// checkedTooLate compares the count only after it allocated.
func checkedTooLate(v xdr.ScVal) ([]byte, error) {
	n := v.MustU32()
	buf := make([]byte, n) // want `IDX002: make sized by n`

	if n > maxItems {
		return nil, errTooBig
	}

	return buf, nil
}

// otherN compares a different variable that has the same name.
func otherN(v xdr.ScVal, items []int) ([]byte, error) {
	n := v.MustU32()
	if n := len(items); n > maxItems {
		return nil, errTooBig
	}

	return make([]byte, n), nil // want `IDX002: make sized by n`
}

// inOneBranch checks the count only on a path the allocation does not take.
func inOneBranch(v xdr.ScVal, strict bool) ([]byte, error) {
	n := v.MustU32()
	if strict {
		if n > maxItems {
			return nil, errTooBig
		}
	}

	return make([]byte, n), nil // want `IDX002: make sized by n`
}

// guarded allocates only inside the branch the check allows.
func guarded(v xdr.ScVal) []byte {
	n := v.MustU32()
	if n <= maxItems {
		return make([]byte, n)
	}

	return nil
}

// switched checks the count in a case expression before the allocation.
func switched(v xdr.ScVal) ([]byte, error) {
	n := v.MustU32()

	switch {
	case n > maxItems:
		return nil, errTooBig
	}

	return make([]byte, n), nil
}

var _ = []any{
	fromU32, fromU64, direct, checked, clamped, fromLen, constant,
	checkedTooLate, otherN, inOneBranch, guarded, switched,
}
