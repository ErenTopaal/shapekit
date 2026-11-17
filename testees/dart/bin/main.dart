// the dart testee for the shapekit conformance runner: answers testee protocol requests
// (docs/testee-protocol.md) with stellar_flutter_sdk. one source, built once per pinned sdk
// version from the pubspec and lock file under versions/.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:shapekit_testee_dart/ops.dart';

Future<void> main() async {
  final lines = stdin.transform(utf8.decoder).transform(const LineSplitter());
  await for (final line in lines) {
    if (line.isEmpty) continue;
    stdout.writeln(jsonEncode(handleLine(line)));
    await stdout.flush();
  }
}
