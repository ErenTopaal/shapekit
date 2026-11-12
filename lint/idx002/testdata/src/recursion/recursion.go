// Package recursion holds seeded IDX002 recursion cases and their clean
// counterparts.
package recursion

import (
	"errors"

	"github.com/stellar/go-stellar-sdk/xdr"
)

const maxDepth = 64

var errTooDeep = errors.New("value nested too deep")

// countAddresses walks nested vectors and maps with nothing to stop it.
func countAddresses(v xdr.ScVal) int { // want `IDX002: countAddresses recurses over xdr.ScVal with no depth bound`
	n := 0

	switch v.Type {
	case xdr.ScValTypeScvAddress:
		return 1
	case xdr.ScValTypeScvVec:
		if vec, ok := v.GetVec(); ok && vec != nil {
			for _, e := range *vec {
				n += countAddresses(e)
			}
		}
	case xdr.ScValTypeScvMap:
		if m, ok := v.GetMap(); ok && m != nil {
			for _, e := range *m {
				n += countAddresses(e.Key) + countAddresses(e.Val)
			}
		}
	}

	return n
}

// countBounded is the fixed version: depth is passed down and checked.
func countBounded(v xdr.ScVal, depth int) (int, error) {
	if depth > maxDepth {
		return 0, errTooDeep
	}

	n := 0

	if vec, ok := v.GetVec(); ok && vec != nil {
		for _, e := range *vec {
			c, err := countBounded(e, depth+1)
			if err != nil {
				return 0, err
			}

			n += c
		}
	}

	return n, nil
}

// remainingBudget counts down instead of up, through a local.
func remainingBudget(v xdr.ScVal, remaining uint) error {
	if remaining == 0 {
		return errTooDeep
	}

	next := remaining - 1

	if vec, ok := v.GetVec(); ok && vec != nil {
		for _, e := range *vec {
			if err := remainingBudget(e, next); err != nil {
				return err
			}
		}
	}

	return nil
}

// loopIndex has a compared and incremented integer, but it is a loop index
// that starts again on every call, so it bounds nothing.
func loopIndex(vec xdr.ScVec) int { // want `IDX002: loopIndex recurses over xdr.ScVec with no depth bound`
	n := 0

	for i := 0; i < len(vec); i++ {
		if inner, ok := vec[i].GetVec(); ok && inner != nil {
			n += loopIndex(*inner)
		}
	}

	return n
}

// walkVal and walkMap recurse through each other.
func walkVal(v xdr.ScVal, out *[]string) { // want `IDX002: walkMap, walkVal recurse over xdr.ScVal with no depth bound`
	if m, ok := v.GetMap(); ok && m != nil {
		walkMap(*m, out)
	}

	if s, ok := v.GetSym(); ok {
		*out = append(*out, string(s))
	}
}

func walkMap(m xdr.ScMap, out *[]string) {
	for _, e := range m {
		walkVal(e.Key, out)
		walkVal(e.Val, out)
	}
}

// walker counts nodes in a field and stops at a limit.
type walker struct {
	nodes int
}

func (w *walker) visit(v xdr.ScVal) error {
	w.nodes++
	if w.nodes > 10_000 {
		return errTooDeep
	}

	if vec, ok := v.GetVec(); ok && vec != nil {
		for _, e := range *vec {
			if err := w.visit(e); err != nil {
				return err
			}
		}
	}

	return nil
}

// predicateHolds evaluates a claim predicate tree without a limit.
func predicateHolds(p xdr.ClaimPredicate, now int64) bool { // want `IDX002: predicateHolds recurses over xdr.ClaimPredicate with no depth bound`
	switch p.Type {
	case xdr.ClaimPredicateTypeClaimPredicateAnd:
		for _, q := range p.MustAndPredicates() {
			if !predicateHolds(q, now) {
				return false
			}
		}

		return true
	case xdr.ClaimPredicateTypeClaimPredicateNot:
		if q, ok := p.GetNotPredicate(); ok && q != nil {
			return !predicateHolds(*q, now)
		}

		return false
	case xdr.ClaimPredicateTypeClaimPredicateBeforeAbsoluteTime:
		return now < int64(p.MustAbsBefore())
	default:
		return true
	}
}

// delegates walks CAP-71 nested delegate signatures through a closure.
func delegates(sigs []xdr.SorobanDelegateSignature) []xdr.ScAddress {
	var out []xdr.ScAddress

	var walk func(s xdr.SorobanDelegateSignature)
	walk = func(s xdr.SorobanDelegateSignature) { // want `IDX002: walk \(closure in delegates\) recurses over xdr.SorobanDelegateSignature with no depth bound`
		out = append(out, s.Address)
		for _, n := range s.NestedDelegates {
			walk(n)
		}
	}

	for _, s := range sigs {
		walk(s)
	}

	return out
}

// delegatesBounded uses a captured depth that the closure steps and checks.
func delegatesBounded(sigs []xdr.SorobanDelegateSignature) ([]xdr.ScAddress, error) {
	var out []xdr.ScAddress

	depth := 0

	var walk func(s xdr.SorobanDelegateSignature) error
	walk = func(s xdr.SorobanDelegateSignature) error {
		depth++
		defer func() { depth-- }()

		if depth > maxDepth {
			return errTooDeep
		}

		out = append(out, s.Address)
		for _, n := range s.NestedDelegates {
			if err := walk(n); err != nil {
				return err
			}
		}

		return nil
	}

	for _, s := range sigs {
		if err := walk(s); err != nil {
			return nil, err
		}
	}

	return out, nil
}

// node is not an xdr type, so recursing over it is not this rule's business.
type node struct {
	children []node
}

func size(n node) int {
	total := 1
	for _, c := range n.children {
		total += size(c)
	}

	return total
}

// flatten has no recursion at all.
func flatten(v xdr.ScVal) []xdr.ScVal {
	if vec, ok := v.GetVec(); ok && vec != nil {
		return *vec
	}

	return nil
}

// trusted is suppressed with a reason, so it is not reported but counted.
//
//indexerlint:ignore IDX002 input is our own fixed configuration, never ledger data
func trusted(v xdr.ScVal) int {
	if vec, ok := v.GetVec(); ok && vec != nil && len(*vec) > 0 {
		return 1 + trusted((*vec)[0])
	}

	return 0
}

var _ = []any{countAddresses, countBounded, remainingBudget, loopIndex, walkVal, predicateHolds, delegates, delegatesBounded, size, flatten, trusted}
