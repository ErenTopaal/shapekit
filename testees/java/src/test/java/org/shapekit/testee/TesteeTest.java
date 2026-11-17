package org.shapekit.testee;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
import org.junit.jupiter.api.Test;
import org.stellar.sdk.xdr.XdrDataInputStream;

/** runs from the repository root (surefire workingDirectory), like the runner starts the testee. */
class TesteeTest {
  private static final String ENTRY =
      "corpus/entries/cap01.bump-sequence/cap01.bump-sequence.p30-bump-negative/";

  private final Ops ops = new Ops(SdkInfo.load(), null);

  @SuppressWarnings("unchecked")
  private Map<String, Object> call(String line) throws Json.ParseError {
    return (Map<String, Object>) Json.parse(Protocol.handle(ops, line));
  }

  @SuppressWarnings("unchecked")
  private static Map<String, Object> map(Object o) {
    return (Map<String, Object>) o;
  }

  private static byte[] envelope() throws IOException {
    return Files.readAllBytes(Path.of(ENTRY + "mainnet.xdr"));
  }

  @Test
  void jsonIsStrictAndKeepsNumberText() throws Json.ParseError {
    assertEquals(new Json.Number("18446744073709551617"), Json.parse("18446744073709551617"));
    assertEquals("{\"a\":[1.50,true,null,\"\\n\"]}", Json.write(Json.parse(" {\"a\" : [1.50, true, null, \"\\n\"]} ")));
    for (String bad : List.of("01", "{'a':1}", "[1,]", "{\"a\":1,\"a\":2}", "1 2", "\"\t\"", "NaN")) {
      assertThrows(Json.ParseError.class, () -> Json.parse(bad), bad);
    }
    assertThrows(Json.ParseError.class, () -> Json.parse("[".repeat(3000) + "]".repeat(3000)));
  }

  @Test
  void helloReadsTheVersionsFromTheJars() throws Json.ParseError {
    Map<String, Object> r = map(call("{\"id\":\"1\",\"op\":\"hello\"}").get("result"));
    assertEquals(SdkInfo.PACKAGE, r.get("sdk"));
    String version = (String) r.get("sdk_version");
    assertTrue(TypesGen.ONLY.containsKey(version), version);
    assertEquals("honoured", r.get("decode_limits"));
    assertTrue(!((List<?>) r.get("ops")).contains("coverage_snapshot"));
  }

  @Test
  void decodeEncodeRoundTripsAndRefusesTrailingBytes() throws Exception {
    byte[] env = envelope();
    String b64 = Base64.getEncoder().encodeToString(env);
    Map<String, Object> ok =
        call(
            "{\"id\":\"2\",\"op\":\"decode_encode\",\"type\":\"TransactionEnvelope\",\"xdr\":\""
                + b64
                + "\",\"depth_limit\":1000,\"len_limit\":"
                + env.length
                + "}");
    assertEquals("ok", ok.get("status"));
    assertEquals(b64, map(ok.get("result")).get("xdr"));

    byte[] longer = java.util.Arrays.copyOf(env, env.length + 4);
    Map<String, Object> trailing =
        call(
            "{\"id\":\"3\",\"op\":\"decode_encode\",\"type\":\"TransactionEnvelope\",\"xdr\":\""
                + Base64.getEncoder().encodeToString(longer)
                + "\",\"depth_limit\":1000,\"len_limit\":"
                + longer.length
                + "}");
    assertEquals("rejected", map(trailing.get("error")).get("class"));
  }

  @Test
  void decodeJsonMatchesTheCorpusAndEncodesBack() throws Exception {
    byte[] env = envelope();
    String b64 = Base64.getEncoder().encodeToString(env);
    Object expected = Json.parse(Files.readString(Path.of(ENTRY + "mainnet.expected.json")));
    Map<String, Object> r =
        call(
            "{\"id\":\"4\",\"op\":\"decode_json\",\"type\":\"TransactionEnvelope\",\"xdr\":\""
                + b64
                + "\",\"depth_limit\":1000,\"len_limit\":"
                + env.length
                + "}");
    // map equality ignores member order, as the protocol's json comparison does
    assertEquals(expected, map(r.get("result")).get("json"));
    Map<String, Object> back =
        call(
            "{\"id\":\"5\",\"op\":\"encode_json\",\"type\":\"TransactionEnvelope\",\"json\":"
                + Json.write(expected)
                + "}");
    assertEquals(b64, map(back.get("result")).get("xdr"));
  }

  @Test
  void requestErrors() throws Json.ParseError {
    assertEquals("", call("not json").get("id"));
    assertEquals("bad-request", map(call("not json").get("error")).get("class"));
    assertEquals("bad-request", map(call("{\"id\":\"6\",\"op\":\"nope\"}").get("error")).get("class"));
    Map<String, Object> b64 =
        call("{\"id\":\"7\",\"op\":\"decode_encode\",\"type\":\"ScVal\",\"xdr\":\"AAA\",\"depth_limit\":1,\"len_limit\":3}");
    assertEquals("bad-request", map(b64.get("error")).get("class"));
    assertEquals("unsupported", call("{\"id\":\"8\",\"op\":\"to_native\",\"xdr\":\"AAAAAQ==\"}").get("status"));
  }

  @Test
  void idParseHandsTheDigitsToGson() throws Json.ParseError {
    Map<String, Object> big = call("{\"id\":\"9\",\"op\":\"id_parse\",\"kind\":\"muxed\",\"input\":\"18446744073709551615\"}");
    assertEquals("18446744073709551615", map(big.get("result")).get("id"));
    Map<String, Object> over = call("{\"id\":\"10\",\"op\":\"id_parse\",\"kind\":\"muxed\",\"input\":\"18446744073709551616\"}");
    assertEquals("rejected", map(over.get("error")).get("class"));
    Map<String, Object> offer = call("{\"id\":\"11\",\"op\":\"id_parse\",\"kind\":\"offer\",\"input\":9007199254740993}");
    assertEquals("9007199254740993", map(offer.get("result")).get("id"));
  }

  /** every type the corpus uses decodes in this sdk version, and every table entry is a decoder. */
  @Test
  void typeTableCoversTheManifest() throws Exception {
    Types types = new Types(SdkInfo.load().version);
    TreeSet<String> used = new TreeSet<>();
    collect(Json.parse(Files.readString(Path.of("corpus/manifest.json"))), used);
    used.add("TransactionMeta");
    List<String> missing = new ArrayList<>();
    for (String name : used) {
      try {
        Class<?> c = types.type(name);
        Types.method(c, "decode", XdrDataInputStream.class, int.class);
        Types.method(c, "fromJson", String.class);
      } catch (Protocol.Unsupported e) {
        missing.add(name);
      }
    }
    assertEquals(List.of(), missing);
    assertTrue(types.size() > 400);
  }

  private static void collect(Object v, TreeSet<String> out) {
    if (v instanceof Map<?, ?> m) {
      for (Map.Entry<?, ?> e : m.entrySet()) {
        if ("xdr_type".equals(e.getKey()) && e.getValue() instanceof String s) {
          out.add(s);
        } else {
          collect(e.getValue(), out);
        }
      }
    } else if (v instanceof List<?> l) {
      for (Object x : l) {
        collect(x, out);
      }
    }
  }
}
