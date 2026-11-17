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
    Object v = r.value();
    r.space();
    if (r.pos != text.length()) {
      throw new ParseError("text after the json value", r.pos);
    }
    return v;
  }

  /**
   * the most arrays and objects one value nests. the deepest request the runner sends holds an
   * expected xdr-json of 1010 levels (a quorum set ledger at the ledger depth limit, the deepest
   * in corpus/ and candidate/) inside the request object, 1011 levels, and the sdk's toJson of a
   * value decoded at the runner's depth limit of 1012 nests no deeper than that limit; 2048 is
   * room above both. the reader and the writer keep their own stack of open containers instead
   * of recursing, so this limit, not the size of the thread stack, decides how deep a value can
   * be.
   */
  static final int MAX_NESTING = 2048;

  private static final class Reader {
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

    /** an open array or object and, for an object, the member name its next value goes to. */
    private static final class Open {
      final List<Object> list;
      final Map<String, Object> map;
      String key;

      Open(List<Object> list, Map<String, Object> map) {
        this.list = list;
        this.map = map;
      }

      Object container() {
        return list != null ? list : map;
      }
    }

    Object value() throws ParseError {
      List<Open> stack = new ArrayList<>();
      while (true) {
        // read one value; an array or object is opened and its first member read on the next turn
        Object v;
        boolean opened = false;
        if (pos >= s.length()) {
          throw new ParseError("unexpected end", pos);
        }
        char c = s.charAt(pos);
        if (c == '[' || c == '{') {
          if (stack.size() >= MAX_NESTING) {
            throw new ParseError("json nested deeper than " + MAX_NESTING, pos);
          }
          pos++;
          space();
          Open o = c == '[' ? new Open(new ArrayList<>(), null) : new Open(null, new LinkedHashMap<>());
          char close = c == '[' ? ']' : '}';
          if (pos < s.length() && s.charAt(pos) == close) {
            pos++;
            v = o.container();
          } else {
            stack.add(o);
            if (o.map != null) {
              memberName(o);
            }
            opened = true;
            v = null;
          }
        } else {
          v = scalar(c);
        }
        if (opened) {
          continue;
        }
        // put the value into the open containers, closing every one that ends here
        while (true) {
          if (stack.isEmpty()) {
            return v;
          }
          Open top = stack.get(stack.size() - 1);
          if (top.list != null) {
            top.list.add(v);
          } else {
            top.map.put(top.key, v);
          }
          space();
          if (pos < s.length() && s.charAt(pos) == ',') {
            pos++;
            space();
            if (top.map != null) {
              memberName(top);
            }
            break;
          }
          expect(top.list != null ? ']' : '}');
          stack.remove(stack.size() - 1);
          v = top.container();
        }
      }
    }

    private void memberName(Open o) throws ParseError {
      space();
      if (pos >= s.length() || s.charAt(pos) != '"') {
        throw new ParseError("expected a member name", pos);
      }
      String key = string();
      space();
      expect(':');
      space();
      if (o.map.containsKey(key)) {
        throw new ParseError("duplicate member " + key, pos);
      }
      o.key = key;
    }

    private Object scalar(char c) throws ParseError {
      switch (c) {
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
    // each open array or object is an iterator over what is left of it, with its closing
    // character, so the writer needs no recursion either
    List<java.util.Iterator<?>> open = new ArrayList<>();
    List<Character> close = new ArrayList<>();
    List<Boolean> first = new ArrayList<>();
    Object next = v;
    boolean pending = true;
    while (true) {
      if (pending) {
        if (next instanceof List<?> l) {
          b.append('[');
          open.add(l.iterator());
          close.add(']');
          first.add(true);
        } else if (next instanceof Map<?, ?> m) {
          b.append('{');
          open.add(m.entrySet().iterator());
          close.add('}');
          first.add(true);
        } else {
          scalar(b, next);
        }
        pending = false;
      }
      if (open.isEmpty()) {
        return b.toString();
      }
      int top = open.size() - 1;
      java.util.Iterator<?> it = open.get(top);
      if (!it.hasNext()) {
        b.append(close.get(top));
        open.remove(top);
        close.remove(top);
        first.remove(top);
        continue;
      }
      if (!first.get(top)) {
        b.append(',');
      }
      first.set(top, false);
      Object item = it.next();
      if (item instanceof Map.Entry<?, ?> e) {
        quote(b, (String) e.getKey());
        b.append(':');
        item = e.getValue();
      }
      next = item;
      pending = true;
    }
  }

  private static void scalar(StringBuilder b, Object v) {
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
    } else {
      throw new IllegalArgumentException(
          "no json form for " + (v == null ? "a java null" : v.getClass().getName()));
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
