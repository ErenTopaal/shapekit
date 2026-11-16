# Seeded faults

Written by `shapekit conformance seed-check`. Each sdk's mutation tool seeds faults into the sdk's encoding and helper code; a sample of them is run against the corpus. A fault is caught when the corpus run on the faulty build differs from the run on the unmodified build.

| sdk | version | tool | faults generated | sampled | caught | timed out | missed | did not build | caught of those that built |
|---|---|---|---|---|---|---|---|---|---|
| rust | 30.0.0 | cargo-mutants 27.1.0 | 11776 | 40 | 7 | 0 | 31 | 2 | 7 of 38 (18.4%) |
| go | v0.9.0 | go-mutesting v2.3.1 | 1924 | 100 | 12 | 0 | 64 | 24 | 12 of 76 (15.8%) |
| js | 17.4.0 | stryker 10.1.1 | 16456 | 100 | 35 | 0 | 65 | 0 | 35 of 100 (35.0%) |

## Missed in rust 30.0.0

| file | line | mutation |
|---|---|---|
| src/generated.rs | 1729 | delete field max_length from struct schemars::schema::StringValidation expression in <impl schemars::JsonSchema for BytesM<MAX>>::json_schema |
| src/generated/account_flags.rs | 96 | replace <impl Name for AccountFlags>::name -> &'static str with "" |
| src/generated/allow_trust_result_code.rs | 76 | replace < with == |
| src/generated/asset.rs | 101 | replace <impl Name for Asset>::name -> &'static str with "" |
| src/generated/claimant.rs | 65 | replace Claimant::name -> &'static str with "" |
| src/generated/clawback_result.rs | 73 | replace < with > |
| src/generated/const/ledger_key_contract_code.rs | 11 | replace LedgerKeyContractCode::const_xdr_len -> usize with 1 |
| src/generated/const/ledger_key_liquidity_pool.rs | 30 | replace LedgerKeyLiquidityPool::const_to_xdr -> [u8; N] with [1; N] |
| src/generated/const/node_id.rs | 30 | replace NodeId::const_to_xdr -> [u8; N] with [0; N] |
| src/generated/const/operation_result.rs | 100 | replace < with == in ConstWriter<'_>::write_type_vec_operation_result |
| src/generated/const/revoke_sponsorship_result_code.rs | 11 | replace RevokeSponsorshipResultCode::const_xdr_len -> usize with 1 |
| src/generated/const/signer.rs | 76 | replace < with > in ConstWriter<'_>::write_type_vec_signer |
| src/generated/const/time_sliced_survey_response_message.rs | 20 | replace TimeSlicedSurveyResponseMessage::const_xdr_len -> usize with 0 |
| src/generated/const/transaction_history_entry.rs | 54 | replace ConstWriter<'_>::write_type_transaction_history_entry with () |
| src/generated/curve25519_public.rs | 42 | replace <impl WriteXdr for Curve25519Public>::write_xdr -> Result<(), Error> with Ok(()) |
| src/generated/encoded_ledger_key.rs | 107 | replace <impl AsRef<[u8]> for EncodedLedgerKey>::as_ref -> &[u8] with Vec::leak(vec![1]) |
| src/generated/extend_footprint_ttl_result_code.rs | 72 | replace ExtendFootprintTtlResultCode::name -> &'static str with "" |
| src/generated/host_function.rs | 56 | replace < with > |
| src/generated/ledger_entry_changes.rs | 72 | replace <impl From<LedgerEntryChanges> for Vec<LedgerEntryChange>>::from -> Self with Default::default() |
| src/generated/ledger_header_flags.rs | 54 | replace += with -= |
| src/generated/revoke_sponsorship_result_code.rs | 127 | delete match arm -5 in <impl TryFrom<i32> for RevokeSponsorshipResultCode>::try_from |
| src/generated/sc_spec_event_param_location_v0.rs | 126 | replace <impl WriteXdr for ScSpecEventParamLocationV0>::write_xdr -> Result<(), Error> with Ok(()) |
| src/generated/soroban_transaction_meta_ext.rs | 81 | replace SorobanTransactionMetaExt::variants -> [i32; Self::_VARIANTS.len()] with [-1; Self::_VARIANTS.len()] |
| src/generated/survey_message_command_type.rs | 37 | replace += with -= |
| src/generated/transaction_ext.rs | 55 | replace < with > |
| src/generated/transaction_result_ext.rs | 52 | replace < with <= |
| src/generated/transaction_result_set.rs | 42 | replace <impl WriteXdr for TransactionResultSet>::write_xdr -> Result<(), Error> with Ok(()) |
| src/generated/type_enum.rs | 1472 | replace += with -= |
| src/generated/type_enum.rs | 3216 | delete match arm "CryptoKeyType" in <impl core::str::FromStr for TypeVariant>::from_str |
| src/generated/upgrade_type.rs | 87 | replace <impl TryFrom<&Vec<u8>> for UpgradeType>::try_from -> Result<Self, Error> with Ok(Default::default()) |
| src/transaction_conversions.rs | 76 | replace <impl From<&Uint256> for MuxedAccount>::from -> Self with Default::default() |

## Missed in go v0.9.0

| file | line | mutation |
|---|---|---|
| strkey/main.go | 118 | if len(raw) < 3 { -> if len(raw) < 4 { |
| strkey/main.go | 64 | const maxEncodedSize = (maxRawSize*8 + 4) / 5 // (8n+4)/5 is the EncodedLen for no padding -> const maxEncodedSize = (maxRawSize*8 + 3) / 5 // (8n+4)/5 is the EncodedLen for no padding |
| strkey/muxed_account.go | 65 | raw = append(raw, b.Bytes()...) -> _, _, _ = raw, raw, b.Bytes |
| strkey/muxed_contract.go | 50 | binary.BigEndian.PutUint64(raw[32:], m.id) -> _, _, _ = binary.BigEndian.PutUint64, raw, m.id |
| strkey/muxed_contract.go | 50 | binary.BigEndian.PutUint64(raw[32:], m.id) -> binary.BigEndian.PutUint64(raw[33:], m.id) |
| xdr/account_entry.go | 44 | if account.Ext.V1 != nil && account.Ext.V1.Ext.V2 != nil { -> if account.Ext.V1 != nil && true { |
| xdr/account_flags.go | 18 | return (accountFlags & AccountFlagsAuthImmutableFlag) != 0 -> return (accountFlags \| AccountFlagsAuthImmutableFlag) != 0 |
| xdr/account_id.go | 101 | return nil ->  |
| xdr/account_id.go | 14 | panic(err) -> _ = err |
| xdr/account_thresholds.go | 8 | return t[1] -> return t[0] |
| xdr/asset.go | 153 | if len(parts) != 2 { -> if len(parts) != 3 { |
| xdr/asset.go | 193 | case length >= 1 && length <= 4: -> case length >= 0 && length <= 4: |
| xdr/asset.go | 193 | case length >= 1 && length <= 4: -> case length >= 1 && length <= 5: |
| xdr/asset.go | 231 | case length >= 1 && length <= 4: -> case length >= 1 && length <= 5: |
| xdr/asset.go | 234 | copy(bytecode[:], byteArray[0:length]) -> _, _, _ = bytecode, byteArray, length |
| xdr/asset.go | 249 | a.MustExtract(&t, &c, &i) -> _, _, _, _ = a.MustExtract, t, c, i |
| xdr/asset.go | 393 | panic(fmt.Errorf("Cannot transform type %v to Asset", a.Type)) -> _, _ = fmt.Errorf, a.Type |
| xdr/asset.go | 481 | return [32]byte{}, err -> _ = err |
| xdr/asset.go | 59 | case length >= 5 && length <= 12: -> case length >= 6 && length <= 12: |
| xdr/claimable_balance_id.go | 56 | return fmt.Errorf("invalid claimable balance id type: %v", payload[0]) -> return fmt.Errorf("invalid claimable balance id type: %v", payload[1]) |
| xdr/db.go | 37 | return errors.New("Invalid value for xdr.AssetType") -> _ = errors.New |
| xdr/db.go | 68 | return errors.New("Invalid value for xdr.Int64") -> _ = errors.New |
| xdr/decorated_signature.go | 31 | hint[i] ^= keyHint[i] -> hint[i] = keyHint[i] |
| xdr/go_string.go | 157 | sb.WriteString(fmt.Sprintf("InflationDest: xdr.MustAddressPtr(%#v),", s.InflationDest.Address())) -> _, _, _ = sb.WriteString, fmt.Sprintf, s.InflationDest.Address |
| xdr/go_string.go | 250 | sb.WriteString("(unknown)") -> _ = sb.WriteString |
| xdr/hash.go | 13 | for i := 0; i < len(s); i++ { -> for i := -1; i < len(s); i++ { |
| xdr/json.go | 138 | parts[i] = converted -> _, _, _ = parts, i, converted |
| xdr/json.go | 162 | payload.AbsBeforeEpoch = &absBeforeEpoch -> _, _ = payload.AbsBeforeEpoch, absBeforeEpoch |
| xdr/json.go | 175 | return nil, err -> _ = err |
| xdr/json.go | 64 | return &iso8601Time{time.Unix(epoch, 0).UTC()} -> return &iso8601Time{time.Unix(epoch, 1).UTC()} |
| xdr/ledger_close_meta_batch.go | 33 | lastSequence := s.LedgerCloseMetas[len(s.LedgerCloseMetas)-1].LedgerSequence() -> lastSequence := s.LedgerCloseMetas[len(s.LedgerCloseMetas)-0].LedgerSequence() |
| xdr/ledger_close_meta_view.go | 160 | return 0, err -> _ = err |
| xdr/ledger_entry.go | 106 | if claimableBalanceEntry.Ext.V == 0 { -> if claimableBalanceEntry.Ext.V == 1 { |
| xdr/ledger_entry.go | 106 | if claimableBalanceEntry.Ext.V == 0 { -> if claimableBalanceEntry.Ext.V == -1 { |
| xdr/ledger_entry.go | 170 | return key, err -> _, _ = key, err |
| xdr/ledger_entry.go | 44 | Selling: 0, -> Selling: 1, |
| xdr/ledger_entry.go | 50 | accountEntry.Ext.V1.Ext.V = 2 -> _ = accountEntry.Ext.V1.Ext.V |
| xdr/ledger_entry.go | 88 | if trustLineEntry.Ext.V == 0 { -> if trustLineEntry.Ext.V == 1 { |
| xdr/ledger_entry.go | 93 | Selling: 0, -> Selling: 1, |
| xdr/ledger_key.go | 111 | *key = nkey -> _, _ = key, nkey |
| xdr/ledger_key.go | 122 | return err -> _ = err |
| xdr/ledger_key.go | 171 | *key = nkey -> _, _ = key, nkey |
| xdr/ledger_key.go | 35 | return l.AccountId.Equals(r.AccountId) && l.Asset.Equals(r.Asset) -> return true && l.Asset.Equals(r.Asset) |
| xdr/ledger_key.go | 72 | *key = nkey -> _, _ = key, nkey |
| xdr/main.go | 200 | return nil, err -> _ = err |
| xdr/main.go | 88 | options.MaxInputLen = hex.DecodedLen(len(data)) -> _, _, _ = options.MaxInputLen, hex.DecodedLen, data |
| xdr/muxed_account.go | 118 | return "", nil ->  |
| xdr/muxed_account.go | 72 | case 69: -> case 68: |
| xdr/muxed_account.go | 79 | if err = muxed.Id.UnmarshalBinary(raw[32:]); err != nil { -> if err = muxed.Id.UnmarshalBinary(raw[31:]); err != nil { |
| xdr/muxed_account.go | 82 | *m, err = NewMuxedAccount(CryptoKeyTypeKeyTypeMuxedEd25519, muxed) -> _, _, _, _, _ = m, err, NewMuxedAccount, CryptoKeyTypeKeyTypeMuxedEd25519, muxed |
| xdr/muxed_account_view.go | 16 | // MustEd25519 rejects any other key type. KEY_TYPE_ED25519 and // PUBLIC_KEY_TYPE_ED25519 are both 0, so the account's XDR is the // AccountId's. _ = v.MustEd2... -> _ = // MustEd25519 rejects any other key type. KEY_TYPE_ED25519 and // PUBLIC_KEY_TYPE_ED25519 are both 0, so the account's XDR is the // AccountId's. v.MustEd2... |
| xdr/operation_trace_result.go | 16 | operationTraceDescription = o.PaymentResult.Code.String() -> _, _ = operationTraceDescription, o.PaymentResult.Code.String |
| xdr/path_payment_result.go | 33 | return 0 ->  |
| xdr/price.go | 85 | return fmt.Errorf("price denominator cannot be 0: %d/%d", p.N, p.D) -> _, _, _ = fmt.Errorf, p.N, p.D |
| xdr/price.go | 87 | if p.N < 0 \|\| p.D < 0 { -> if p.N < 0 \|\| false { |
| xdr/signer_key.go | 151 | *skey, err = NewSignerKey(keytype, signer) -> _, _, _, _, _ = skey, err, NewSignerKey, keytype, signer |
| xdr/stellar_value.go | 73 | return 0, fmt.Errorf("close time out of range: %d", uint64(t)) -> return 1, fmt.Errorf("close time out of range: %d", uint64(t)) |
| xdr/stellar_value.go | 75 | return int64(t) * 1000, nil //nolint:gosec // bounded above -> return int64(t) * 999, nil //nolint:gosec // bounded above |
| xdr/transaction_envelope.go | 119 | return nil ->  |
| xdr/transaction_meta.go | 42 | return nil, nil ->  |
| xdr/trust_line_asset.go | 41 | panic(err) -> _ = err |
| xdr/trust_line_asset.go | 69 | return err -> _ = err |
| xdr/view_validate.go | 14 | return viewErrTrailingBytes(uint32(n), len(v)-n) //nolint:gosec // view offsets are uint32 throughout -> return viewErrTrailingBytes(uint32(n), len(v)+n) //nolint:gosec // view offsets are uint32 throughout |
| xdr/xdrstream.go | 270 | return err -> _ = err |

## Released js faults

| issue | reproductions | caught |
|---|---|---|
| https://github.com/stellar/js-stellar-sdk/issues/1785 | js-1785-non-canonical-ids | yes |
| https://github.com/stellar/js-stellar-sdk/issues/1789 | js-1789-colliding-map-keys | yes |
| https://github.com/stellar/js-stellar-sdk/pull/1766 | js-1766-issuer-order, js-1766-map-key-order | yes |

## Missed in js 17.4.0

| file | line | mutation |
|---|---|---|
| lib/esm/base/asset.js | 142 | BlockStatement -> {} |
| lib/esm/base/auth.js | 468 | ConditionalExpression -> case "public_key": |
| lib/esm/base/claimant.js | 43 | ConditionalExpression -> true |
| lib/esm/base/invocation.js | 32 | EqualityOperator -> preimage.type !== "contractIdPreimageFromAddress" |
| lib/esm/base/memo.js | 10 | StringLiteral -> "" |
| lib/esm/base/operation.js | 441 | StringLiteral -> "" |
| lib/esm/base/operations/invoke_host_function.js | 89 | ConditionalExpression -> true |
| lib/esm/base/operations/set_options.js | 74 | ConditionalExpression -> true |
| lib/esm/base/operations/set_trustline_flags.js | 29 | ConditionalExpression -> false |
| lib/esm/base/transaction.js | 163 | StringLiteral -> "" |
| lib/esm/base/transaction_base.js | 180 | BooleanLiteral -> keypair.verify(this.hash(), signatureBytes) |
| lib/esm/base/transaction_base.js | 209 | EqualityOperator -> preimage.length >= 64 |
| lib/esm/base/transaction_builder.js | 117 | ObjectLiteral -> {} |
| lib/esm/base/transaction_builder.js | 156 | StringLiteral -> `` |
| lib/esm/base/transaction_builder.js | 372 | BlockStatement -> {} |
| lib/esm/base/transaction_builder.js | 55 | ArrayDeclaration -> [] |
| lib/esm/base/transaction_builder.js | 601 | ObjectLiteral -> {} |
| lib/esm/base/transaction_builder.js | 809 | ObjectLiteral -> {} |
| lib/esm/base/util/continued_fraction.js | 53 | BlockStatement -> {} |
| lib/esm/base/util/operations.js | 19 | BlockStatement -> {} |
| lib/esm/base/util/operations.js | 60 | StringLiteral -> "" |
| lib/esm/xdr/generated/asset-type.js | 28 | StringLiteral -> "" |
| lib/esm/xdr/generated/bucket-entry.js | 96 | ObjectLiteral -> {} |
| lib/esm/xdr/generated/change-trust-result.js | 19 | StringLiteral -> "" |
| lib/esm/xdr/generated/claim-claimable-balance-result.js | 15 | StringLiteral -> "" |
| lib/esm/xdr/generated/contract-cost-type.js | 213 | StringLiteral -> "" |
| lib/esm/xdr/generated/contract-event-type.js | 8 | StringLiteral -> "" |
| lib/esm/xdr/generated/create-account-result.js | 34 | BlockStatement -> {} |
| lib/esm/xdr/generated/end-sponsoring-future-reserves-result-code.js | 14 | StringLiteral -> "" |
| lib/esm/xdr/generated/end-sponsoring-future-reserves-result.js | 10 | StringLiteral -> "" |
| lib/esm/xdr/generated/error-code.js | 26 | BlockStatement -> {} |
| lib/esm/xdr/generated/extend-footprint-ttl-result.js | 79 | UnaryOperator -> +1 |
| lib/esm/xdr/generated/hash-id-preimage.js | 141 | ObjectLiteral -> {} |
| lib/esm/xdr/generated/ledger-entry-change-type.js | 6 | StringLiteral -> "" |
| lib/esm/xdr/generated/ledger-key.js | 262 | BlockStatement -> {} |
| lib/esm/xdr/generated/liquidity-pool-withdraw-result.js | 33 | BlockStatement -> {} |
| lib/esm/xdr/generated/manage-buy-offer-result.js | 149 | StringLiteral -> "" |
| lib/esm/xdr/generated/manage-sell-offer-result.js | 177 | BlockStatement -> {} |
| lib/esm/xdr/generated/manage-sell-offer-result.js | 34 | StringLiteral -> "" |
| lib/esm/xdr/generated/memo.js | 32 | BlockStatement -> {} |
| lib/esm/xdr/generated/operation-result-tr.js | 226 | BlockStatement -> {} |
| lib/esm/xdr/generated/operation-type.js | 25 | StringLiteral -> "" |
| lib/esm/xdr/generated/path-payment-strict-receive-result-code.js | 54 | StringLiteral -> "" |
| lib/esm/xdr/generated/persisted-scp-state.js | 29 | BlockStatement -> {} |
| lib/esm/xdr/generated/restore-footprint-result.js | 20 | StringLiteral -> "" |
| lib/esm/xdr/generated/sc-meta-kind.js | 10 | StringLiteral -> "" |
| lib/esm/xdr/generated/sc-spec-entry.js | 160 | BlockStatement -> {} |
| lib/esm/xdr/generated/sc-spec-type-def.js | 409 | StringLiteral -> "" |
| lib/esm/xdr/generated/sc-spec-udt-union-case-v0.js | 10 | ConditionalExpression -> false |
| lib/esm/xdr/generated/scp-quorum-set.js | 34 | ArrowFunction -> () => undefined |
| lib/esm/xdr/generated/scp-statement-confirm.js | 30 | BlockStatement -> {} |
| lib/esm/xdr/generated/scp-statement-pledges.js | 19 | StringLiteral -> "" |
| lib/esm/xdr/generated/scp-statement-pledges.js | 86 | BlockStatement -> {} |
| lib/esm/xdr/generated/scp-statement-type.js | 6 | StringLiteral -> "" |
| lib/esm/xdr/generated/send-more-extended.js | 25 | BlockStatement -> {} |
| lib/esm/xdr/generated/soroban-transaction-meta-ext.js | 20 | StringLiteral -> "" |
| lib/esm/xdr/generated/stellar-value-type.js | 46 | BlockStatement -> {} |
| lib/esm/xdr/generated/stored-transaction-set.js | 76 | StringLiteral -> "" |
| lib/esm/xdr/generated/survey-message-response-type.js | 12 | BlockStatement -> {} |
| lib/esm/xdr/generated/transaction-signature-payload-tagged-transaction.js | 10 | ConditionalExpression -> false |
| lib/esm/xdr/generated/trust-line-entry-extension-v2-ext.js | 30 | StringLiteral -> `` |
| lib/esm/xdr/values/bigint-parts.js | 24 | ConditionalExpression -> false |
| lib/esm/xdr/values/xdr-string.js | 168 | ConditionalExpression -> false |
| lib/esm/xdr/values/xdr-string.js | 174 | ArithmeticOperator -> code - 65 - 10 |
| lib/esm/xdr/values/xdr-value.js | 84 | StringLiteral -> "" |
