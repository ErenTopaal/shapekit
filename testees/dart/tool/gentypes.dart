// writes lib/tables_gen.dart for the dart testee: the stellar_flutter_sdk class behind every
// type the xdr schema defines (schema/xdr/<tag>/xdr.json), read from the sdk source the
// package config resolves.
//
// a schema type matches the class Xdr<name> whose name equals it without regard to case
// (SCVal and ScVal are XdrSCVal). where the sdk names a type another way, the class is listed
// in `aliases` below with the reason. the class must declare the static codec itself: dart
// does not inherit statics, so a class whose decode, encode or fromXdrJsonValue lives only on
// its base class is not used under the subclass's name. schema types without a class are
// written to `unmappedTypes`, so the testee answers unsupported for them with the reason.
//
// usage, from a directory whose .dart_tool/package_config.json resolves the sdk:
//   dart run tool/gentypes.dart <schema xdr.json> <out file>

import 'dart:convert';
import 'dart:io';

// schema name (lower case) -> sdk class, where the sdk's name differs. each was checked to be
// the same xdr shape and the same SEP-51 rendering in the sdk source.
const aliases = {
  // lib/src/xdr/xdr_account_entry_v1.dart and its neighbours: the sdk drops "Extension"
  'accountentryextensionv1': 'XdrAccountEntryV1',
  'accountentryextensionv1ext': 'XdrAccountEntryV1Ext',
  'accountentryextensionv2': 'XdrAccountEntryV2',
  'accountentryextensionv2ext': 'XdrAccountEntryV2Ext',
  'accountentryextensionv3': 'XdrAccountEntryV3',
  'claimablebalanceentryextensionv1': 'XdrClaimableBalanceEntryExtV1',
  'claimablebalanceentryextensionv1ext': 'XdrClaimableBalanceEntryExtV1Ext',
  'ledgerentryextensionv1': 'XdrLedgerEntryV1',
  'ledgerentryextensionv1ext': 'XdrLedgerEntryV1Ext',
  // lib/src/xdr/xdr_asset_alpha_num4.dart, xdr_asset_alpha_num12.dart
  'alphanum4': 'XdrAssetAlphaNum4',
  'alphanum12': 'XdrAssetAlphaNum12',
  // lib/src/xdr/xdr_manage_offer_result.dart: one class for both offer operations, whose
  // result unions and codes have the same values and, prefix stripped, the same SEP-51 names
  'managesellofferresult': 'XdrManageOfferResult',
  'managebuyofferresult': 'XdrManageOfferResult',
  'managesellofferresultcode': 'XdrManageOfferResultCode',
  'managebuyofferresultcode': 'XdrManageOfferResultCode',
  // lib/src/xdr/xdr_path_payment_result_success.dart: one class for both path payments
  'pathpaymentstrictreceiveresultsuccess': 'XdrPathPaymentResultSuccess',
  'pathpaymentstrictsendresultsuccess': 'XdrPathPaymentResultSuccess',
  // lib/src/xdr/xdr_revoke_sponsorship_signer.dart
  'revokesponsorshipopsigner': 'XdrRevokeSponsorshipSigner',
  // lib/src/xdr/xdr_signed_payload.dart
  'signerkeyed25519signedpayload': 'XdrSignedPayload',
  // the overlay's Error, which stellar-xdr names SError; the request carries SError
  'serror': 'XdrError',
};

void main(List<String> args) {
  if (args.length != 2) {
    stderr.writeln(
      'usage: dart run tool/gentypes.dart <schema xdr.json> <out file>',
    );
    exit(2);
  }
  final schema =
      jsonDecode(File(args[0]).readAsStringSync()) as Map<String, dynamic>;
  final names = <String>[
    for (final d in schema['definitions'] as List)
      if (const {'struct', 'union', 'enum', 'typedef'}.contains(d['kind']))
        d['name'] as String,
  ];
  if (names.isEmpty) throw StateError('${args[0]} defines no type');

  final sdk = _sdkDir();
  final version = RegExp(
    r'^version:\s*(\S+)',
    multiLine: true,
  ).firstMatch(File('${sdk.path}/pubspec.yaml').readAsStringSync())!.group(1)!;
  final classes = <String, Set<String>>{};
  for (final f in Directory(
    '${sdk.path}/lib/src/xdr',
  ).listSync().whereType<File>()) {
    if (!f.path.endsWith('.dart')) continue;
    final text = f.readAsStringSync();
    final starts = RegExp(
      r'^class (Xdr\w+)',
      multiLine: true,
    ).allMatches(text).toList();
    for (var i = 0; i < starts.length; i++) {
      final end = i + 1 < starts.length ? starts[i + 1].start : text.length;
      final body = text.substring(starts[i].start, end);
      classes[starts[i].group(1)!] = {
        if (RegExp(r'static \w+ decode\(\s*XdrDataInputStream').hasMatch(body))
          'decode',
        if (RegExp(
          r'static void encode\(\s*XdrDataOutputStream',
        ).hasMatch(body))
          'encode',
        if (RegExp(r'static \w+ fromXdrJsonValue\(').hasMatch(body)) 'fromJson',
        if (RegExp(r'toXdrJsonValue\(\)').hasMatch(body)) 'toJson',
      };
    }
  }
  final byLower = <String, String>{};
  for (final c in classes.keys) {
    final key = c.substring(3).toLowerCase();
    if (byLower.containsKey(key))
      throw StateError('$key names ${byLower[key]} and $c');
    byLower[key] = c;
  }

  final mapped = <String, String>{};
  final unmapped = <String, String>{};
  for (final name in names..sort()) {
    final key = name.toLowerCase();
    final cls = aliases[key] ?? byLower[key];
    if (cls == null) {
      unmapped[key] =
          'stellar_flutter_sdk $version has no class for the xdr type $name';
      continue;
    }
    final have = classes[cls]!;
    final missing = {'decode', 'encode', 'fromJson'}.difference(have);
    if (missing.isNotEmpty) {
      unmapped[key] =
          'stellar_flutter_sdk $version class $cls for $name declares no static '
          '${missing.join(', ')} of its own';
      continue;
    }
    mapped[key] = cls;
  }
  for (final a in aliases.keys) {
    if (!names.any((n) => n.toLowerCase() == a) && a != 'serror') {
      throw StateError('alias $a names no schema type');
    }
  }
  final serror = aliases['serror']!;
  if (classes[serror]?.containsAll({'decode', 'encode', 'fromJson'}) ?? false) {
    mapped['serror'] = serror;
    unmapped.remove('serror');
  }

  final out = StringBuffer()
    ..writeln(
      '// Code generated by tool/gentypes.dart from ${args[0].split('/').reversed.take(2).toList().reversed.join('/')} and',
    )
    ..writeln('// stellar_flutter_sdk $version. DO NOT EDIT.')
    ..writeln('// ignore_for_file: implementation_imports')
    ..writeln()
    ..writeln("import 'package:stellar_flutter_sdk/src/xdr/xdr.dart';")
    ..writeln()
    ..writeln(
      '/// The codec of one xdr type: the sdk class\'s own static decode, encode and',
    )
    ..writeln('/// fromXdrJsonValue, and the instance\'s toXdrJsonValue.')
    ..writeln('class XdrType {')
    ..writeln('  const XdrType(this.decode, this.encode, this.fromJson);')
    ..writeln('  final Object Function(XdrDataInputStream) decode;')
    ..writeln('  final void Function(XdrDataOutputStream, Object) encode;')
    ..writeln('  final Object Function(Object?) fromJson;')
    ..writeln(
      '  Object? toJson(Object value) => (value as dynamic).toXdrJsonValue();',
    )
    ..writeln('}')
    ..writeln()
    ..writeln(
      '/// Every xdr type with an sdk class, by its name in lower case.',
    )
    ..writeln('final Map<String, XdrType> xdrTypesByLowerName = {');
  for (final e in mapped.entries) {
    final c = e.value;
    out.writeln(
      "  '${e.key}': XdrType((s) => $c.decode(s), (s, v) => $c.encode(s, v as $c), (j) => $c.fromXdrJsonValue(j)),",
    );
  }
  out
    ..writeln('};')
    ..writeln()
    ..writeln(
      '/// Why the schema types without a usable class have none, by name in lower case.',
    )
    ..writeln('const Map<String, String> unmappedTypesByLowerName = {');
  for (final e in unmapped.entries) {
    out.writeln("  '${e.key}': ${jsonEncode(e.value)},");
  }
  out.writeln('};');
  File(args[1]).writeAsStringSync(out.toString());
  stdout.writeln(
    '${mapped.length} types mapped, ${unmapped.length} without a class',
  );
}

Directory _sdkDir() {
  final config = jsonDecode(
    File('.dart_tool/package_config.json').readAsStringSync(),
  );
  for (final p in config['packages'] as List) {
    if (p['name'] == 'stellar_flutter_sdk') {
      var uri = Uri.parse(p['rootUri'] as String);
      if (!uri.isAbsolute)
        uri = Directory.current.uri.resolve('.dart_tool/').resolveUri(uri);
      return Directory.fromUri(uri);
    }
  }
  throw StateError('the package config does not resolve stellar_flutter_sdk');
}
