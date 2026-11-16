package main

// id_parse: go-stellar-sdk reads textual 64-bit ids where it decodes horizon's json, through
// its own response types: horizon.Offer.ID (`json:"id,string"`, int64) for offer ids and
// horizon.Transaction.AccountMuxedID (`json:"account_muxed_id,omitempty,string"`, uint64) for
// muxed ids. The runner's input goes into that field exactly as it arrived, string or number,
// and encoding/json decodes it the way the sdk does for horizon responses.

import (
	"encoding/json"
	"strconv"
	"strings"

	"github.com/stellar/go-stellar-sdk/protocols/horizon"
)

func idParse(req request) (map[string]any, error) {
	kind, err := req.str("kind")
	if err != nil {
		return nil, err
	}
	input, err := req.raw("input")
	if err != nil {
		return nil, err
	}
	// the `,string` fields read an id from a json string only
	if trimmed := strings.TrimSpace(string(input)); trimmed == "" || trimmed[0] != '"' {
		return nil, unsupported("go-stellar-sdk's horizon types read ids from json strings only " +
			"(`json:\",string\"`), so a json number has no sdk api")
	}
	switch kind {
	case "offer":
		var offer horizon.Offer
		if err := sdkCall("json.Unmarshal into horizon.Offer", func() error {
			return json.Unmarshal([]byte(`{"id":`+string(input)+`}`), &offer)
		}); err != nil {
			return nil, err
		}
		return map[string]any{"id": strconv.FormatInt(offer.ID, 10)}, nil
	case "muxed":
		var tx horizon.Transaction
		if err := sdkCall("json.Unmarshal into horizon.Transaction", func() error {
			return json.Unmarshal([]byte(`{"account_muxed_id":`+string(input)+`}`), &tx)
		}); err != nil {
			return nil, err
		}
		return map[string]any{"id": strconv.FormatUint(tx.AccountMuxedID, 10)}, nil
	}
	return nil, badRequest("id_parse kind must be muxed or offer, not %q", kind)
}
