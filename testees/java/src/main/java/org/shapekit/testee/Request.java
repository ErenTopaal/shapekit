package org.shapekit.testee;

import java.util.Base64;
import java.util.Map;
import org.shapekit.testee.Protocol.BadRequest;

/** one request; json numbers keep their text, so id_parse sees the digits the runner wrote. */
final class Request {
  final String op;
  final Map<String, Object> members;

  Request(String op, Map<String, Object> members) {
    this.op = op;
    this.members = members;
  }

  boolean has(String name) {
    return members.containsKey(name);
  }

  Object raw(String name) {
    Object v = members.get(name);
    if (v == null) {
      throw new BadRequest(op + " needs member " + name);
    }
    return v;
  }

  String str(String name) {
    if (!(raw(name) instanceof String s)) {
      throw new BadRequest(name + " must be a string");
    }
    return s;
  }

  byte[] bytes(String name) {
    return base64(str(name), name);
  }

  static byte[] base64(String text, String what) {
    // java's decoder also reads unpadded input; the protocol's base64 is always padded
    if (text.length() % 4 != 0) {
      throw new BadRequest(what + " is not standard base64");
    }
    try {
      return Base64.getDecoder().decode(text);
    } catch (IllegalArgumentException e) {
      throw new BadRequest(what + " is not standard base64: " + e.getMessage());
    }
  }

  /** an integer member that fits a java int and is at least min. */
  int integer(String name, int min) {
    if (!(raw(name) instanceof Json.Number n)) {
      throw new BadRequest(name + " must be an integer");
    }
    int v;
    try {
      v = Integer.parseInt(n.text());
    } catch (NumberFormatException e) {
      throw new BadRequest(name + " must be an integer that fits 32 bits, got " + n.text());
    }
    if (v < min) {
      throw new BadRequest(name + " must be at least " + min + ", got " + v);
    }
    return v;
  }

  boolean bool(String name) {
    if (!(raw(name) instanceof Boolean b)) {
      throw new BadRequest(name + " must be a boolean");
    }
    return b;
  }
}
