package org.shapekit.testee;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * strict rfc 8259 json for the protocol. the testee has its own reader and writer so that the
 * sdk's gson, whose version changes with the sdk, is only ever exercised through the sdk's own
 * api. values are Map (insertion order), List, String, Boolean, Number (the source text), and
 * NULL.
 */
final class Json {
  private Json() {}

  /** the json null. */
  static final Object NULL = new Object();

  /** a json number as the text it was written with, so 64-bit values keep every digit. */
  record Number(String text) {}

  static final class ParseError extends Exception {
    private static final long serialVersionUID = 1L;

    ParseError(String message, int at) {
      super(message + " at offset " + at);
    }
  }

  static Object parse(String text) throws ParseError {
    Reader r = new Reader(text);
    r.space();
    Object v = r.value(0);
    r.space();
    if (r.pos != text.length()) {
      throw new ParseError("text after the json value", r.pos);
    }
    return v;
  }

  private static final class Reader {
    // the testee runs on java's default thread stack, as an sdk caller does, and 5000 levels
    // of this reader overflow it. the deepest expected xdr-json in corpus/ and candidate/
    // nests 1010 levels (a quorum set ledger at the depth limit), so twice that is room enough
    private static final int MAX_NESTING = 2048;
    private final String s;
    int pos;

    Reader(String s) {
      this.s = s;
    }

    void space() {
      while (pos < s.length()) {
        char c = s.charAt(pos);
        if (c != ' ' && c != '\t' && c != '\n' && c != '\r') {
          return;
        }
        pos++;
      }
    }

    Object value(int depth) throws ParseError {
      if (depth > MAX_NESTING) {
        throw new ParseError("json nested deeper than " + MAX_NESTING, pos);
      }
      if (pos >= s.length()) {
        throw new ParseError("unexpected end", pos);
      }
      char c = s.charAt(pos);
      switch (c) {
        case '{':
          return object(depth);
        case '[':
          return array(depth);
        case '"':
          return string();
        case 't':
          return literal("true", Boolean.TRUE);
        case 'f':
          return literal("false", Boolean.FALSE);
        case 'n':
          return literal("null", NULL);
        default:
          if (c == '-' || (c >= '0' && c <= '9')) {
            return number();
          }
          throw new ParseError("unexpected character " + c, pos);
      }
    }

    Object literal(String word, Object v) throws ParseError {
      if (!s.startsWith(word, pos)) {
        throw new ParseError("bad literal", pos);
      }
      pos += word.length();
      return v;
    }

    Map<String, Object> object(int depth) throws ParseError {
      Map<String, Object> m = new LinkedHashMap<>();
      pos++;
      space();
      if (pos < s.length() && s.charAt(pos) == '}') {
        pos++;
        return m;
      }
      while (true) {
        space();
        if (pos >= s.length() || s.charAt(pos) != '"') {
          throw new ParseError("expected a member name", pos);
        }
        String key = string();
        space();
        expect(':');
        space();
        if (m.containsKey(key)) {
          throw new ParseError("duplicate member " + key, pos);
        }
        m.put(key, value(depth + 1));
        space();
        if (pos < s.length() && s.charAt(pos) == ',') {
          pos++;
          continue;
        }
        expect('}');
        return m;
      }
    }

    List<Object> array(int depth) throws ParseError {
      List<Object> l = new ArrayList<>();
      pos++;
      space();
      if (pos < s.length() && s.charAt(pos) == ']') {
        pos++;
        return l;
      }
      while (true) {
        space();
        l.add(value(depth + 1));
        space();
        if (pos < s.length() && s.charAt(pos) == ',') {
          pos++;
          continue;
        }
        expect(']');
        return l;
      }
    }

    void expect(char c) throws ParseError {
      if (pos >= s.length() || s.charAt(pos) != c) {
        throw new ParseError("expected " + c, pos);
      }
      pos++;
    }

    String string() throws ParseError {
      pos++;
      StringBuilder b = new StringBuilder();
      while (true) {
        if (pos >= s.length()) {
          throw new ParseError("unterminated string", pos);
        }
        char c = s.charAt(pos++);
        if (c == '"') {
          return b.toString();
        }
        if (c < 0x20) {
          throw new ParseError("control character in string", pos - 1);
        }
        if (c != '\\') {
          b.append(c);
          continue;
        }
        if (pos >= s.length()) {
          throw new ParseError("unterminated escape", pos);
        }
        char e = s.charAt(pos++);
        switch (e) {
          case '"', '\\', '/' -> b.append(e);
          case 'b' -> b.append('\b');
          case 'f' -> b.append('\f');
          case 'n' -> b.append('\n');
          case 'r' -> b.append('\r');
          case 't' -> b.append('\t');
          case 'u' -> {
            if (pos + 4 > s.length()) {
              throw new ParseError("short unicode escape", pos);
            }
            try {
              b.append((char) Integer.parseInt(s.substring(pos, pos + 4), 16));
            } catch (NumberFormatException ex) {
              throw new ParseError("bad unicode escape", pos);
            }
            pos += 4;
          }
          default -> throw new ParseError("bad escape \\" + e, pos - 1);
        }
      }
    }

    Number number() throws ParseError {
      int start = pos;
      if (s.charAt(pos) == '-') {
        pos++;
      }
      if (pos < s.length() && s.charAt(pos) == '0') {
        pos++;
      } else if (!digits()) {
        throw new ParseError("bad number", start);
      }
      if (pos < s.length() && s.charAt(pos) == '.') {
        pos++;
        if (!digits()) {
          throw new ParseError("bad fraction", start);
        }
      }
      if (pos < s.length() && (s.charAt(pos) == 'e' || s.charAt(pos) == 'E')) {
        pos++;
        if (pos < s.length() && (s.charAt(pos) == '+' || s.charAt(pos) == '-')) {
          pos++;
        }
        if (!digits()) {
          throw new ParseError("bad exponent", start);
        }
      }
      return new Number(s.substring(start, pos));
    }

    boolean digits() {
      int start = pos;
      while (pos < s.length() && s.charAt(pos) >= '0' && s.charAt(pos) <= '9') {
        pos++;
      }
      return pos > start;
    }
  }

  static String write(Object v) {
    StringBuilder b = new StringBuilder();
    write(b, v);
    return b.toString();
  }

  // every value written was read by the parser above or built by the testee, so it nests no
  // deeper than MAX_NESTING
  @SuppressWarnings("unchecked")
  private static void write(StringBuilder b, Object v) {
    if (v == NULL) {
      b.append("null");
    } else if (v instanceof Boolean x) {
      b.append(x.booleanValue());
    } else if (v instanceof Number n) {
      b.append(n.text());
    } else if (v instanceof Integer || v instanceof Long) {
      b.append(v);
    } else if (v instanceof String x) {
      quote(b, x);
    } else if (v instanceof List<?> l) {
      b.append('[');
      for (int i = 0; i < l.size(); i++) {
        if (i > 0) {
          b.append(',');
        }
        write(b, l.get(i));
      }
      b.append(']');
    } else if (v instanceof Map<?, ?> m) {
      b.append('{');
      boolean first = true;
      for (Map.Entry<String, Object> e : ((Map<String, Object>) m).entrySet()) {
        if (!first) {
          b.append(',');
        }
        first = false;
        quote(b, e.getKey());
        b.append(':');
        write(b, e.getValue());
      }
      b.append('}');
    } else {
      throw new IllegalArgumentException("no json form for " + v.getClass().getName());
    }
  }

  static void quote(StringBuilder b, String s) {
    b.append('"');
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      switch (c) {
        case '"' -> b.append("\\\"");
        case '\\' -> b.append("\\\\");
        case '\n' -> b.append("\\n");
        case '\r' -> b.append("\\r");
        case '\t' -> b.append("\\t");
        case '\b' -> b.append("\\b");
        case '\f' -> b.append("\\f");
        default -> {
          if (c < 0x20) {
            b.append(String.format("\\u%04x", (int) c));
          } else {
            b.append(c);
          }
        }
      }
    }
    b.append('"');
  }
}
