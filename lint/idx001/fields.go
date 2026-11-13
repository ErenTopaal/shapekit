package idx001

import (
	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable"
)

// trustedPartial are the "partial" rows of fields.toml that IDX001 treats
// like "no" rows. Core checks something about them, but not the property an
// indexer relies on: a declared auth entry or footprint key says what the
// submitter allowed, not what happened, and a signed payload has a variable
// length that core does not cap at 32 bytes. Other partial rows (asset
// codes, home domains, contract function names) are left out: core does
// check the part that matters for storing them, and flagging every use of
// them would bury the findings that matter.
var trustedPartial = map[string]bool{
	"signer_key.signed_payload.payload":                 true,
	"tx.soroban.footprint":                              true,
	"op.invoke_host_function.auth":                      true,
	"auth.credentials.address.address":                  true,
	"auth.credentials.address.nonce":                    true,
	"auth.credentials.address.signature":                true,
	"auth.credentials.address_with_delegates.delegates": true,
	"auth.root_invocation":                              true,
}

// carriers are methods that are not listed as accessors in fields.toml but
// hand out an unvalidated field inside their result: MuxedAccount.Address
// and GetAddress render the mux id into an M strkey, SignerKey.Address
// renders a signed payload into a P strkey. Each inherits the rows of the
// listed accessor named on the right.
var carriers = map[fieldtable.Accessor]fieldtable.Accessor{
	{Type: "MuxedAccount", Member: "Address"}:    {Type: "MuxedAccount", Member: "GetId"},
	{Type: "MuxedAccount", Member: "GetAddress"}: {Type: "MuxedAccount", Member: "GetId"},
	{Type: "SignerKey", Member: "Address"}:       {Type: "SignerKey", Member: "GetEd25519SignedPayload"},
}

// unmuxers return the base account and drop the mux id.
var unmuxers = map[fieldtable.Accessor]bool{
	{Type: "MuxedAccount", Member: "ToAccountId"}: true,
}

// sourceRow maps every accessor of a selected row, and every carrier, to the
// first selected row that lists it.
var sourceRow = buildSources()

func selected(f fieldtable.Field) bool {
	return f.ValidatedByCore == "no" || trustedPartial[f.ID]
}

func buildSources() map[fieldtable.Accessor]fieldtable.Field {
	out := map[fieldtable.Accessor]fieldtable.Field{}

	for _, f := range fieldtable.Fields {
		if !selected(f) {
			continue
		}

		for _, a := range f.Accessors {
			if _, ok := out[a]; !ok {
				out[a] = f
			}
		}
	}

	for c, via := range carriers {
		if f, ok := out[via]; ok {
			out[c] = f
		}
	}

	return out
}
