package walletbackend

import (
	"encoding/hex"
	"fmt"

	"github.com/stellar/go-stellar-sdk/strkey"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// column kinds decide which strkey forms a stored value may take: account
// columns (AddressBytea) hold G or C keys, signer columns (SignerKeyBytea at
// fixed commits, AddressBytea before) hold G, T, X or P keys.
type columnKind int

const (
	accountColumn columnKind = iota
	signerColumn
)

var versionNames = map[strkey.VersionByte]string{
	strkey.VersionByteAccountID:        "G",
	strkey.VersionByteSeed:             "S",
	strkey.VersionByteMuxedAccount:     "M",
	strkey.VersionByteHashTx:           "T",
	strkey.VersionByteHashX:            "X",
	strkey.VersionByteSignedPayload:    "P",
	strkey.VersionByteContract:         "C",
	strkey.VersionByteLiquidityPool:    "L",
	strkey.VersionByteClaimableBalance: "B",
	strkey.VersionByteMuxedContract:    "W",
}

// decodeStored turns stored bytes (version byte then payload, exactly as
// wallet-backend wrote them) into a strkey. The bytes are never padded or
// cut: a 33-byte value under a muxed or signed-payload version byte is the
// corruption the harness looks for, so it comes back as an error.
func decodeStored(b []byte, kind columnKind) (string, error) {
	if len(b) < 2 {
		return "", fmt.Errorf("%d stored bytes cannot hold a version byte and a key", len(b))
	}
	v := strkey.VersionByte(b[0])
	name, known := versionNames[v]
	if !known {
		return "", fmt.Errorf("version byte 0x%02x is not a strkey version", b[0])
	}
	allowed := false
	switch kind {
	case accountColumn:
		allowed = v == strkey.VersionByteAccountID || v == strkey.VersionByteContract
	case signerColumn:
		allowed = v == strkey.VersionByteAccountID || v == strkey.VersionByteHashTx ||
			v == strkey.VersionByteHashX || v == strkey.VersionByteSignedPayload
	}
	payload := b[1:]
	enc, err := strkey.Encode(v, payload)
	if err != nil {
		return "", fmt.Errorf("%s key with a %d-byte payload: %w", name, len(payload), err)
	}
	// encode does not check the payload shape; decoding does (sep-23 lengths and
	// the cap-40 signed payload structure)
	if _, _, err := strkey.DecodeAny(enc); err != nil {
		return "", fmt.Errorf("%s key with a %d-byte payload does not decode: %w", name, len(payload), err)
	}
	if !allowed {
		return "", fmt.Errorf("a %s key is not a canonical value for this column", name)
	}
	return enc, nil
}

// rawAddress decodes one stored address column into the export's raw form and
// the strkey (nil when it does not decode).
func rawAddress(column string, b []byte, kind columnKind) (wire.RawAddress, *string) {
	ra := wire.RawAddress{Column: column, Hex: hex.EncodeToString(b)}
	s, err := decodeStored(b, kind)
	if err != nil {
		ra.Error = err.Error()
		return ra, nil
	}
	ra.Strkey = s
	return ra, &s
}
