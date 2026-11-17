import 'dart:convert';

import 'package:shapekit_testee_dart/ops.dart';
import 'package:test/test.dart';

Map<String, Object?> ask(Map<String, Object?> request) => handleLine(jsonEncode(request));

void main() {
  test('hello names the linked sdk', () {
    final r = ask({'id': '1', 'op': 'hello'});
    expect(r['status'], 'ok');
    expect((r['result'] as Map)['sdk'], 'stellar_flutter_sdk');
  });

  test('an ScVal round trips', () {
    final r = ask({
      'id': '2',
      'op': 'decode_encode',
      'type': 'ScVal',
      'xdr': 'AAAAAwAAAAc=',
      'depth_limit': 1000,
      'len_limit': 8,
    });
    expect((r['result'] as Map)['xdr'], 'AAAAAwAAAAc=');
  });

  test('trailing bytes are a refusal', () {
    final r = ask({
      'id': '3',
      'op': 'decode_encode',
      'type': 'ScVal',
      'xdr': 'AAAAAwAAAAcA',
      'depth_limit': 1000,
      'len_limit': 9,
    });
    expect((r['error'] as Map)['class'], 'rejected');
  });

  test('type names match without regard to case', () {
    final r = ask({
      'id': '4',
      'op': 'decode_json',
      'type': 'SCVal',
      'xdr': 'AAAAAwAAAAc=',
      'depth_limit': 1000,
      'len_limit': 8,
    });
    expect((r['result'] as Map)['json'], {'u32': 7});
  });

  test('an account strkey round trips', () {
    const g = 'GD7777777777777777777777777777777777777777777777777773DB';
    final d = ask({'id': '5', 'op': 'strkey_decode', 'text': g});
    final e = ask({
      'id': '6',
      'op': 'strkey_encode',
      'kind': 'account',
      'payload': (d['result'] as Map)['payload'],
    });
    expect((e['result'] as Map)['text'], g);
  });

  test('a type the sdk lacks is unsupported', () {
    final r = ask({
      'id': '7',
      'op': 'decode_encode',
      'type': 'ScSymbol',
      'xdr': 'AAAAAA==',
      'depth_limit': 1000,
      'len_limit': 4,
    });
    expect(r['status'], 'unsupported');
  });
}
