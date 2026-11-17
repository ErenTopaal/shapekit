// the testee protocol operations on stellar_flutter_sdk.
//
// the package's public entry point (package:stellar_flutter_sdk/stellar_flutter_sdk.dart) also
// exports two smart account files that import package:flutter/services.dart, which needs
// dart:ui and so cannot run on the dart vm. the testee imports the libraries that entry point
// exports, one by one, without those two; every class and function used is part of the
// package's exported api.
// ignore_for_file: implementation_imports

import 'dart:convert';
import 'dart:typed_data';

import 'package:stellar_flutter_sdk/src/asset_type_credit_alphanum4.dart';
import 'package:stellar_flutter_sdk/src/asset_type_pool_share.dart';
import 'package:stellar_flutter_sdk/src/assets.dart';
import 'package:stellar_flutter_sdk/src/key_pair.dart';
import 'package:stellar_flutter_sdk/src/manage_sell_offer_operation.dart';
import 'package:stellar_flutter_sdk/src/network.dart';
import 'package:stellar_flutter_sdk/src/soroban/sc_val_host_order.dart';
import 'package:stellar_flutter_sdk/src/soroban/soroban_auth.dart';
import 'package:stellar_flutter_sdk/src/stellar_sdk.dart';
import 'package:stellar_flutter_sdk/src/transaction.dart';
import 'package:stellar_flutter_sdk/src/util.dart';
import 'package:stellar_flutter_sdk/src/xdr/xdr.dart';

import 'tables_gen.dart';

const int protocolVersion = 2;

const List<String> ops = [
  'decode_encode',
  'decode_json',
  'encode_json',
  'strkey_decode',
  'strkey_encode',
  'asset_order',
  'pool_id',
  'map_sort',
  'id_parse',
  'to_native',
  'address_from_scval',
  'tx_hash',
  'balance_id',
  'contract_id',
];

class BadRequest implements Exception {
  BadRequest(this.message);
  final String message;
}

class Unsupported implements Exception {
  Unsupported(this.reason);
  final String reason;
}

/// A refusal the testee itself reports for the sdk, with the sdk's own words where there are
/// some.
class Refused implements Exception {
  Refused(this.message);
  final String message;
}

Map<String, Object?> _error(String id, String cls, String message) => {
  'id': id,
  'status': 'error',
  'error': {'class': cls, 'message': message},
};

/// A stack overflow, running out of memory, a type error (a null check on a value the sdk
/// assumed present among them) or a call to a member that does not exist is the sdk crashing.
/// Every other throwable, exception or error, is the sdk's refusal: the sdk throws
/// ArgumentError, RangeError, FormatException and plain exceptions on purpose for bad input.
bool isCrash(Object e) =>
    e is StackOverflowError ||
    e is OutOfMemoryError ||
    e is TypeError ||
    e is NoSuchMethodError;

Map<String, Object?> handleLine(String line) {
  Object? decoded;
  try {
    decoded = jsonDecode(line);
  } catch (e) {
    return _error('', 'bad-request', 'request is not json: $e');
  }
  if (decoded is! Map<String, dynamic> || decoded['id'] is! String) {
    return _error(
      '',
      'bad-request',
      'request is not an object with a string id',
    );
  }
  final req = decoded;
  final id = req['id'] as String;
  try {
    final op = req['op'];
    if (op is! String) throw BadRequest('request has no op');
    return {'id': id, 'status': 'ok', 'result': run(op, req)};
  } on BadRequest catch (e) {
    return _error(id, 'bad-request', e.message);
  } on Unsupported catch (e) {
    return {'id': id, 'status': 'unsupported', 'reason': e.reason};
  } on Refused catch (e) {
    return _error(id, 'rejected', e.message);
  } catch (e) {
    return _error(
      id,
      isCrash(e) ? 'panic' : 'rejected',
      '${e.runtimeType}: $e',
    );
  }
}

Map<String, Object?> run(String op, Map<String, dynamic> req) {
  switch (op) {
    case 'hello':
      return {
        'protocol': protocolVersion,
        'sdk': 'stellar_flutter_sdk',
        'sdk_version': StellarSDK.versionNumber,
        'components': <String, String>{},
        'ops': ops,
        // the decoders take no limits; the only bound is the sdk's own nesting cap of 128 for
        // delegate signatures (XdrDataInputStream.maxRecursiveDecodeDepth)
        'decode_limits': 'ignored',
      };
    case 'decode_encode':
      final t = _type(req);
      final value = _decode(t, _bytes(req, 'xdr'));
      return {'xdr': base64.encode(_encode(t, value))};
    case 'decode_json':
      final t = _type(req);
      final value = _decode(t, _bytes(req, 'xdr'));
      return {'json': t.toJson(value)};
    case 'encode_json':
      final t = _type(req);
      if (!req.containsKey('json'))
        throw BadRequest('encode_json needs member json');
      return {'xdr': base64.encode(_encode(t, t.fromJson(req['json'])))};
    case 'strkey_decode':
      return _strkeyDecode(req);
    case 'strkey_encode':
      return _strkeyEncode(req);
    case 'asset_order':
      return {'order': _assetOrder(_asset(req, 'a'), _asset(req, 'b'))};
    case 'pool_id':
      return _poolId(req);
    case 'map_sort':
      return _mapSort(req);
    case 'id_parse':
      return _idParse(req);
    case 'to_native':
      return _toNative(req);
    case 'address_from_scval':
      final val = _scval(_bytes(req, 'xdr'));
      return {'text': _addressText(Address.fromXdrSCVal(val))};
    case 'tx_hash':
      return _txHash(req);
    case 'balance_id':
      return _balanceId(req);
    case 'contract_id':
      return _contractId(req);
    case 'map_validate':
      throw Unsupported(
        'stellar_flutter_sdk has no ScMap validation: it orders keys the host way '
        '(compareScValHostOrder, sortedScMap in lib/src/soroban/sc_val_host_order.dart) but '
        'offers no check of a map it is given',
      );
    case 'coverage_snapshot':
    case 'coverage_new':
      throw Unsupported('the dart testee has no coverage build');
    default:
      throw BadRequest('unknown op $op');
  }
}

String _string(Map<String, dynamic> req, String name) {
  final v = req[name];
  if (v is! String) throw BadRequest('$name must be a string');
  return v;
}

Uint8List _bytes(Map<String, dynamic> req, String name) {
  try {
    return base64.decode(_string(req, name));
  } on FormatException catch (e) {
    throw BadRequest('$name is not base64: ${e.message}');
  }
}

XdrType _type(Map<String, dynamic> req) {
  final name = _string(req, 'type');
  final t = xdrTypesByLowerName[name.toLowerCase()];
  if (t == null) {
    throw Unsupported(
      unmappedTypesByLowerName[name.toLowerCase()] ??
          'stellar_flutter_sdk ${StellarSDK.versionNumber} has no class for the xdr type $name',
    );
  }
  return t;
}

XdrType _known(String name) => xdrTypesByLowerName[name.toLowerCase()]!;

/// Decodes the whole input: bytes left over after the value are a refusal, as the protocol
/// requires of every decoder.
Object _decode(XdrType t, Uint8List bytes) {
  final stream = XdrDataInputStream(bytes);
  final value = t.decode(stream);
  final left = stream.fileLength! - stream.offset!;
  if (left != 0) throw Refused('$left trailing bytes after the value');
  return value;
}

Uint8List _encode(XdrType t, Object value) {
  final out = XdrDataOutputStream();
  t.encode(out, value);
  return Uint8List.fromList(out.bytes);
}

XdrSCVal _scval(Uint8List bytes) => _decode(_known('ScVal'), bytes) as XdrSCVal;

Map<String, Object?> _strkeyDecode(Map<String, dynamic> req) {
  final String text;
  if (req.containsKey('text') && !req.containsKey('text_base64')) {
    text = _string(req, 'text');
  } else if (req.containsKey('text_base64') && !req.containsKey('text')) {
    final raw = _bytes(req, 'text_base64');
    try {
      text = utf8.decode(raw);
    } on FormatException {
      throw Unsupported(
        'the strkey decoders of stellar_flutter_sdk take a dart String; bytes that are not '
        'utf-8 have no String form',
      );
    }
  } else {
    throw BadRequest('strkey_decode needs exactly one of text and text_base64');
  }
  if (text.isEmpty) throw Refused('empty strkey');
  switch (text[0]) {
    case 'G':
      return _kind('account', StrKey.decodeStellarAccountId(text));
    case 'M':
      return _kind('muxed_account', StrKey.decodeStellarMuxedAccountId(text));
    case 'S':
      return _kind('seed', StrKey.decodeStellarSecretSeed(text));
    case 'T':
      return _kind('pre_auth_tx', StrKey.decodePreAuthTx(text));
    case 'X':
      return _kind('hash_x', StrKey.decodeSha256Hash(text));
    case 'C':
      return _kind('contract', StrKey.decodeContractId(text));
    case 'L':
      return _kind('liquidity_pool', StrKey.decodeLiquidityPoolId(text));
    case 'B':
      return _kind('claimable_balance', StrKey.decodeClaimableBalanceId(text));
    case 'W':
      return _kind('muxed_contract', StrKey.decodeMuxedContractId(text));
    case 'P':
      final p = StrKey.decodeXdrSignedPayload(text);
      final payload = p.payload.dataValue;
      final raw = BytesBuilder()
        ..add(p.ed25519.uint256)
        ..add(_be32(payload.length))
        ..add(payload)
        ..add(Uint8List((4 - payload.length % 4) % 4));
      return _kind('signed_payload', raw.toBytes());
    default:
      throw Refused('no strkey decoder for a string starting with ${text[0]}');
  }
}

Uint8List _be32(int n) => Uint8List(4)..buffer.asByteData().setUint32(0, n);

Map<String, Object?> _kind(String kind, Uint8List payload) => {
  'kind': kind,
  'payload': base64.encode(payload),
};

Map<String, Object?> _strkeyEncode(Map<String, dynamic> req) {
  final kind = _string(req, 'kind');
  final p = _bytes(req, 'payload');
  final String text;
  switch (kind) {
    case 'account':
      text = StrKey.encodeStellarAccountId(p);
    case 'muxed_account':
      text = StrKey.encodeStellarMuxedAccountId(p);
    case 'seed':
      text = StrKey.encodeStellarSecretSeed(p);
    case 'pre_auth_tx':
      text = StrKey.encodePreAuthTx(p);
    case 'hash_x':
      text = StrKey.encodeSha256Hash(p);
    case 'contract':
      text = StrKey.encodeContractId(p);
    case 'liquidity_pool':
      text = StrKey.encodeLiquidityPoolId(p);
    case 'claimable_balance':
      text = StrKey.encodeClaimableBalanceId(p);
    case 'muxed_contract':
      text = StrKey.encodeMuxedContractId(p);
    case 'signed_payload':
      // the encoder takes an XdrSignedPayload: the key, then the payload its length names
      if (p.length < 36)
        throw Refused('a signed payload needs a key and a length');
      final n = p.buffer.asByteData(p.offsetInBytes).getUint32(32);
      if (36 + n > p.length)
        throw Refused('the payload length runs past the input');
      text = StrKey.encodeXdrSignedPayload(
        XdrSignedPayload(
          XdrUint256(p.sublist(0, 32)),
          XdrDataValue(p.sublist(36, 36 + n)),
        ),
      );
    default:
      throw BadRequest('unknown strkey kind $kind');
  }
  return {'text': text};
}

Asset _asset(Map<String, dynamic> req, String name) =>
    Asset.fromXdr(_decode(_known('Asset'), _bytes(req, name)) as XdrAsset);

bool _orders(Asset a, Asset b) {
  try {
    AssetTypePoolShare(assetA: a, assetB: b);
    return true;
  } catch (e) {
    if (isCrash(e)) rethrow;
    return false;
  }
}

/// The sdk's only asset ordering is the check in AssetTypePoolShare's constructor, which
/// refuses an assetA that sorts after assetB (lib/src/asset_type_pool_share.dart). Two native
/// assets it refuses in either order, so they have no answer.
int _assetOrder(Asset a, Asset b) {
  final ab = _orders(a, b);
  final ba = _orders(b, a);
  if (ab && ba) return 0;
  if (ab) return -1;
  if (ba) return 1;
  throw Refused('AssetTypePoolShare refuses both orders of these assets');
}

/// The sdk has no pool id function. As the protocol says for that case, the testee hashes the
/// LiquidityPoolParameters the sdk's own encoder writes, for pairs the sdk's own ordering puts
/// strictly in order.
Map<String, Object?> _poolId(Map<String, dynamic> req) {
  final a = _asset(req, 'a');
  final b = _asset(req, 'b');
  final fee = req['fee'];
  if (fee is! int) throw BadRequest('fee must be an integer');
  AssetTypePoolShare(assetA: a, assetB: b);
  if (_orders(b, a)) throw Refused('the assets are equal');
  final params = XdrLiquidityPoolParameters(
    XdrLiquidityPoolType.LIQUIDITY_POOL_CONSTANT_PRODUCT,
  );
  params.constantProduct = XdrLiquidityPoolConstantProductParameters(
    a.toXdr(),
    b.toXdr(),
    XdrInt32(fee),
  );
  final out = XdrDataOutputStream();
  XdrLiquidityPoolParameters.encode(out, params);
  return {'pool_id': Util.bytesToHex(Util.hash(Uint8List.fromList(out.bytes)))};
}

Map<String, Object?> _mapSort(Map<String, dynamic> req) {
  final entries = req['entries'];
  if (entries is! List) throw BadRequest('entries must be an array');
  final list = <XdrSCMapEntry>[];
  for (final e in entries) {
    if (e is! Map) throw BadRequest('an entry is not an object');
    list.add(
      XdrSCMapEntry(
        _scval(_bytes(e.cast(), 'key')),
        _scval(_bytes(e.cast(), 'val')),
      ),
    );
  }
  final sorted = sortedScMap(list);
  return {'xdr': base64.encode(_encode(_known('ScVal'), sorted))};
}

/// Offer ids: ManageSellOfferOperationBuilder.setOfferId takes the id as text, and the
/// operation's xdr carries what the sdk made of it. Muxed ids: MuxedAccount takes a BigInt the
/// caller has already parsed, so there is no textual muxed id api.
Map<String, Object?> _idParse(Map<String, dynamic> req) {
  final kind = _string(req, 'kind');
  final input = req['input'];
  if (kind == 'muxed') {
    throw Unsupported(
      'stellar_flutter_sdk takes a muxed id only as a BigInt (MuxedAccount(accountId, id), '
      'lib/src/muxed_account.dart); it reads no muxed id from text or a json number',
    );
  }
  if (kind != 'offer') throw BadRequest('unknown id kind $kind');
  if (input is! String) {
    throw Unsupported(
      'ManageSellOfferOperationBuilder.setOfferId takes text only',
    );
  }
  final issuer = KeyPair.fromAccountId(
    'GD7777777777777777777777777777777777777777777777777773DB',
  );
  final op = ManageSellOfferOperationBuilder(
    Asset.NATIVE,
    AssetTypeCreditAlphaNum4('ABC', issuer.accountId),
    '1',
    '1',
  ).setOfferId(input).build();
  final body = op.toOperationBody();
  final out = XdrDataOutputStream();
  XdrManageSellOfferOp.encode(out, body.manageSellOfferOp!);
  // offerID is the op's last field (ManageSellOfferOp in Stellar-transaction.x), so the
  // encoding ends with the eight bytes of the int64 the sdk wrote
  final bytes = Uint8List.fromList(out.bytes);
  final id = bytes.buffer.asByteData().getInt64(bytes.length - 8);
  return {'id': id.toString()};
}

Map<String, Object?> _toNative(Map<String, dynamic> req) {
  final val = _scval(_bytes(req, 'xdr'));
  final native = val.toNative();
  // the documented refusal: a value with no native form comes back as the XdrSCVal itself
  // (XdrSCVal.toNative, lib/src/xdr/xdr_sc_val.dart)
  if (native is XdrSCVal) {
    throw Refused('toNative returned the XdrSCVal itself: no native form');
  }
  return {'native': _describe(native)};
}

Object? _describe(Object? v) {
  if (v == null) return {'type': 'null'};
  if (v is bool) return {'type': 'bool', 'value': v};
  if (v is int || v is BigInt)
    return {'type': 'integer', 'value': v.toString()};
  if (v is String) return {'type': 'string', 'value': v};
  if (v is Uint8List) return {'type': 'bytes', 'value': base64.encode(v)};
  if (v is List) return {'type': 'array', 'items': v.map(_describe).toList()};
  if (v is Map) {
    return {
      'type': 'map',
      'entries': [
        for (final e in v.entries)
          {'key': _describe(e.key), 'value': _describe(e.value)},
      ],
    };
  }
  if (v is Address)
    return {'type': 'object', 'class': 'Address', 'text': _addressText(v)};
  if (v is XdrSCVal) {
    return {
      'type': 'object',
      'class': 'XdrSCVal',
      'text': base64.encode(_encode(_known('ScVal'), v)),
    };
  }
  return {
    'type': 'object',
    'class': v.runtimeType.toString(),
    'text': v.toString(),
  };
}

/// The strkey of an address, from the fields Address.fromXdr fills, with the sdk's encoders:
/// contract, claimable balance and liquidity pool ids are held as hex.
String _addressText(Address a) {
  switch (a.type) {
    case Address.TYPE_ACCOUNT:
      return a.accountId!;
    case Address.TYPE_CONTRACT:
      return StrKey.encodeContractIdHex(a.contractId!);
    case Address.TYPE_MUXED_ACCOUNT:
      return a.muxedAccountId!;
    case Address.TYPE_CLAIMABLE_BALANCE:
      return StrKey.encodeClaimableBalanceIdHex(a.claimableBalanceId!);
    case Address.TYPE_LIQUIDITY_POOL:
      return StrKey.encodeLiquidityPoolIdHex(a.liquidityPoolId!);
    case Address.TYPE_MUXED_CONTRACT:
      return a.muxedContractId!;
    default:
      throw Refused('an address of type ${a.type}');
  }
}

Network _network(Map<String, dynamic> req) =>
    Network(_string(req, 'network_passphrase'));

XdrTransactionEnvelope _envelope(Map<String, dynamic> req) =>
    _decode(_known('TransactionEnvelope'), _bytes(req, 'xdr'))
        as XdrTransactionEnvelope;

Map<String, Object?> _txHash(Map<String, dynamic> req) {
  final tx = AbstractTransaction.fromEnvelopeXdr(_envelope(req));
  final network = _network(req);
  final inner = req['inner'];
  if (inner != null && inner is! bool)
    throw BadRequest('inner must be a boolean');
  if (inner == true) {
    if (tx is! FeeBumpTransaction)
      throw BadRequest('inner on an envelope that is no fee bump');
    return {'hash': Util.bytesToHex(tx.innerTransaction.hash(network))};
  }
  return {'hash': Util.bytesToHex(tx.hash(network))};
}

/// The sdk has no balance id function, so the testee hashes the HashIDPreimage of the operation
/// (ENVELOPE_TYPE_OP_ID) that the sdk's own encoder writes, from the sdk's transaction object.
Map<String, Object?> _balanceId(Map<String, dynamic> req) {
  var tx = AbstractTransaction.fromEnvelopeXdr(_envelope(req));
  if (tx is FeeBumpTransaction) tx = tx.innerTransaction;
  final t = tx as Transaction;
  final index = req['op_index'];
  if (index is! int || index < 0)
    throw BadRequest('op_index must be a non-negative integer');
  if (index >= t.operations.length)
    throw Refused('no operation at index $index');
  final body = t.operations[index].toOperationBody();
  if (body.discriminant != XdrOperationType.CREATE_CLAIMABLE_BALANCE) {
    throw Refused(
      'the operation at index $index does not create a claimable balance',
    );
  }
  final source = KeyPair.fromAccountId(t.sourceAccount.ed25519AccountId);
  final preimage = XdrHashIDPreimage(XdrEnvelopeType.ENVELOPE_TYPE_OP_ID);
  preimage.operationID = XdrHashIDPreimageOperationID(
    XdrAccountID(source.xdrPublicKey),
    XdrSequenceNumber(t.sequenceNumber),
    XdrUint32(index),
  );
  final out = XdrDataOutputStream();
  XdrHashIDPreimage.encode(out, preimage);
  final hash = Util.hash(Uint8List.fromList(out.bytes));
  return {'balance_id': '00000000${Util.bytesToHex(hash)}'};
}

/// Address.deriveContractId for address preimages; the sdk has no function for asset
/// preimages, so for those the testee hashes the HashIDPreimage its encoder writes, the way
/// deriveContractId does.
Map<String, Object?> _contractId(Map<String, dynamic> req) {
  final preimage =
      _decode(_known('ContractIdPreimage'), _bytes(req, 'preimage'))
          as XdrContractIDPreimage;
  final network = _network(req);
  if (preimage.discriminant ==
      XdrContractIDPreimageType.CONTRACT_ID_PREIMAGE_FROM_ADDRESS) {
    final from = preimage.fromAddress!;
    final id = Address.deriveContractId(
      deployer: Address.fromXdr(from.address),
      salt: from.salt.uint256,
      network: network,
    );
    return {'contract_id': Util.bytesToHex(StrKey.decodeContractId(id))};
  }
  final hp = XdrHashIDPreimage(XdrEnvelopeType.ENVELOPE_TYPE_CONTRACT_ID);
  hp.contractID = XdrHashIDPreimageContractID(
    XdrHash(network.networkId),
    preimage,
  );
  final out = XdrDataOutputStream();
  XdrHashIDPreimage.encode(out, hp);
  return {
    'contract_id': Util.bytesToHex(Util.hash(Uint8List.fromList(out.bytes))),
  };
}
