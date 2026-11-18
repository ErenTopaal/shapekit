package main

import (
	"encoding/base64"
	"encoding/hex"
	"strings"

	"github.com/stellar/go-stellar-sdk/hash"
	"github.com/stellar/go-stellar-sdk/network"
	"github.com/stellar/go-stellar-sdk/txnbuild"
	"github.com/stellar/go-stellar-sdk/xdr"
)

// the ops whose answers the network writes into a ledger: transaction hashes, claimable
// balance ids and contract ids

func envelopeMember(req request) (xdr.TransactionEnvelope, error) {
	var env xdr.TransactionEnvelope
	data, err := req.bytes("xdr")
	if err != nil {
		return env, err
	}
	depth, maxLen, err := requestLimits(req)
	if err != nil {
		return env, err
	}
	return env, decodeBytes(data, &env, depth, maxLen)
}

// txHash hashes with network.HashTransactionInEnvelope, or for inner: true the fee bump's
// inner transaction with network.HashTransaction.
func txHash(req request) (map[string]any, error) {
	passphrase, err := req.str("network_passphrase")
	if err != nil {
		return nil, err
	}
	var inner bool
	if _, ok := req.fields["inner"]; ok {
		if err := req.get("inner", "a boolean", &inner); err != nil {
			return nil, err
		}
	}
	env, err := envelopeMember(req)
	if err != nil {
		return nil, err
	}
	var h [32]byte
	if inner {
		if env.Type != xdr.EnvelopeTypeEnvelopeTypeTxFeeBump {
			return nil, badRequest("inner is only defined for a fee bump envelope")
		}
		err = sdkCall("network.HashTransaction", func() error {
			var e error
			h, e = network.HashTransaction(env.FeeBump.Tx.InnerTx.V1.Tx, passphrase)
			return e
		})
	} else {
		err = sdkCall("network.HashTransactionInEnvelope", func() error {
			var e error
			h, e = network.HashTransactionInEnvelope(env, passphrase)
			return e
		})
	}
	if err != nil {
		return nil, err
	}
	return map[string]any{"hash": hex.EncodeToString(h[:])}, nil
}

// balanceID reads the envelope with txnbuild.TransactionFromXDR and asks
// Transaction.ClaimableBalanceID, on the inner transaction of a fee bump.
func balanceID(req request) (map[string]any, error) {
	if _, err := req.str("network_passphrase"); err != nil {
		return nil, err
	}
	index, err := req.nonNegativeInt("op_index")
	if err != nil {
		return nil, err
	}
	data, err := req.bytes("xdr")
	if err != nil {
		return nil, err
	}
	// TransactionFromXDR takes no limits, so the envelope is read under the request's limits
	// first; bytes those refuse never reach it
	if _, err := envelopeMember(req); err != nil {
		return nil, err
	}
	var id string
	err = sdkCall("txnbuild.Transaction.ClaimableBalanceID", func() error {
		gt, e := txnbuild.TransactionFromXDR(base64.StdEncoding.EncodeToString(data))
		if e != nil {
			return e
		}
		tx, ok := gt.Transaction()
		if !ok {
			fb, _ := gt.FeeBump()
			tx = fb.InnerTransaction()
		}
		id, e = tx.ClaimableBalanceID(index)
		return e
	})
	if err != nil {
		return nil, err
	}
	return map[string]any{"balance_id": strings.ToLower(id)}, nil
}

// contractID uses xdr.Asset.ContractID for an asset preimage. An address preimage has no sdk
// function of its own, so its id is hash.Hash of the sdk's encoding of the HashIdPreimage,
// the way Asset.ContractID builds it.
func contractID(req request) (map[string]any, error) {
	passphrase, err := req.str("network_passphrase")
	if err != nil {
		return nil, err
	}
	data, err := req.bytes("preimage")
	if err != nil {
		return nil, err
	}
	depth, maxLen, err := requestLimits(req)
	if err != nil {
		return nil, err
	}
	var preimage xdr.ContractIdPreimage
	if err := decodeBytes(data, &preimage, depth, maxLen); err != nil {
		return nil, err
	}
	var id [32]byte
	if preimage.Type == xdr.ContractIdPreimageTypeContractIdPreimageFromAsset {
		err = sdkCall("xdr.Asset.ContractID", func() error {
			var e error
			id, e = preimage.FromAsset.ContractID(passphrase)
			return e
		})
	} else {
		err = sdkCall("xdr.HashIdPreimage.MarshalBinary", func() error {
			full := xdr.HashIdPreimage{
				Type: xdr.EnvelopeTypeEnvelopeTypeContractId,
				ContractId: &xdr.HashIdPreimageContractId{
					NetworkId:          xdr.Hash(network.ID(passphrase)),
					ContractIdPreimage: preimage,
				},
			}
			b, e := full.MarshalBinary()
			if e != nil {
				return e
			}
			id = hash.Hash(b)
			return nil
		})
	}
	if err != nil {
		return nil, err
	}
	return map[string]any{"contract_id": hex.EncodeToString(id[:])}, nil
}
