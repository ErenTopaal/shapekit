package org.shapekit.testee;

import java.lang.reflect.Method;
import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.shapekit.testee.Protocol.BadRequest;
import org.shapekit.testee.Protocol.SdkFailure;
import org.shapekit.testee.Protocol.Unsupported;
import org.stellar.sdk.AbstractTransaction;
import org.stellar.sdk.FeeBumpTransaction;
import org.stellar.sdk.LiquidityPool;
import org.stellar.sdk.MuxedAccount;
import org.stellar.sdk.Network;
import org.stellar.sdk.StrKey;
import org.stellar.sdk.Transaction;
import org.stellar.sdk.Util;
import org.stellar.sdk.responses.OfferResponse;
import org.stellar.sdk.responses.TransactionResponse;
import org.stellar.sdk.scval.Scv;
import org.stellar.sdk.xdr.Asset;
import org.stellar.sdk.xdr.ContractIDPreimage;
import org.stellar.sdk.xdr.ContractIDPreimageType;
import org.stellar.sdk.xdr.EnvelopeType;
import org.stellar.sdk.xdr.Hash;
import org.stellar.sdk.xdr.HashIDPreimage;
import org.stellar.sdk.xdr.Int32;
import org.stellar.sdk.xdr.LiquidityPoolConstantProductParameters;
import org.stellar.sdk.xdr.LiquidityPoolParameters;
import org.stellar.sdk.xdr.LiquidityPoolType;
import org.stellar.sdk.xdr.SCVal;
import org.stellar.sdk.xdr.TransactionEnvelope;
import org.stellar.sdk.xdr.XdrElement;

/** the testee protocol operations on java-stellar-sdk's public api. */
final class Ops {
  // decode depth for the inputs of ops whose requests carry no limits (docs/testee-protocol.md,
  // hello decode_limits)
  private static final int DEFAULT_DEPTH = 1000;

  // the base account of muxed ids: 32 bytes of 0x3a, the key the textual-id vectors use
  private static final String MUXED_BASE =
      "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ";

  private static final HexFormat HEX = HexFormat.of();

  /** a strkey kind, its prefix and the StrKey codec suffix (decodeX, encodeX). */
  private record Kind(String kind, char prefix, String codec) {}

  private static final List<Kind> KINDS =
      List.of(
          new Kind("account", 'G', "Ed25519PublicKey"),
          new Kind("muxed_account", 'M', "Med25519PublicKey"),
          new Kind("seed", 'S', "Ed25519SecretSeed"),
          new Kind("pre_auth_tx", 'T', "PreAuthTx"),
          new Kind("hash_x", 'X', "Sha256Hash"),
          new Kind("signed_payload", 'P', "SignedPayload"),
          new Kind("contract", 'C', "Contract"),
          new Kind("liquidity_pool", 'L', "LiquidityPool"),
          new Kind("claimable_balance", 'B', "ClaimableBalance"),
          // 5.0.0 has no W codec; the method is looked up, so one source serves both releases
          new Kind("muxed_contract", 'W', "MuxedContract"));

  private final SdkInfo info;
  private final Types types;
  private final Coverage coverage;

  Ops(SdkInfo info, Coverage coverage) {
    this.info = info;
    this.types = new Types(info.version);
    this.coverage = coverage;
  }

  List<String> supported() {
    List<String> ops =
        new ArrayList<>(
            List.of(
                "decode_encode",
                "decode_json",
                "encode_json",
                "strkey_decode",
                "strkey_encode",
                "asset_order",
                "pool_id",
                "map_sort",
                "id_parse",
                "tx_hash",
                "balance_id",
                "contract_id"));
    if (coverage != null) {
      ops.add("coverage_snapshot");
      ops.add("coverage_new");
    }
    return ops;
  }

  Map<String, Object> run(Request req) {
    switch (req.op) {
      case "hello":
        return hello();
      case "decode_encode":
        return decodeEncode(req);
      case "decode_json":
        return decodeJson(req);
      case "encode_json":
        return encodeJson(req);
      case "strkey_decode":
        return strkeyDecode(req);
      case "strkey_encode":
        return strkeyEncode(req);
      case "asset_order":
        return assetOrder(req);
      case "pool_id":
        return poolId(req);
      case "map_sort":
        return mapSort(req);
      case "map_validate":
        // scval/ScvComparator.java: the class and compareScVal are package-private, and no
        // public api of the sdk checks the key order of an SCMap
        throw new Unsupported(
            SdkInfo.PACKAGE
                + " "
                + info.version
                + " has no ScMap validation: ScvComparator, which Scv.toMap sorts with, is"
                + " package-private and nothing public checks an SCMap's key order");
      case "id_parse":
        return idParse(req);
      case "to_native":
        // scval/Scv.java: only typed converters (fromInt32, fromMap, fromVec, ...) that each
        // refuse any other arm; nothing turns an arbitrary SCVal into a java value
        throw new Unsupported(
            SdkInfo.PACKAGE
                + " "
                + info.version
                + " has no native projection of an ScVal: org.stellar.sdk.scval.Scv has one"
                + " typed fromX converter per arm and none that dispatches on the arm");
      case "tx_hash":
        return txHash(req);
      case "balance_id":
        return balanceId(req);
      case "contract_id":
        return contractId(req);
      case "coverage_snapshot":
        return coverage().snapshot();
      case "coverage_new":
        return coverage().fresh();
      default:
        throw new BadRequest("unknown op " + req.op);
    }
  }

  private Coverage coverage() {
    if (coverage == null) {
      throw new Unsupported(
          "coverage is collected only when the testee runs with the jacoco agent and"
              + " SHAPEKIT_COVERAGE=1");
    }
    return coverage;
  }

  private Map<String, Object> hello() {
    Map<String, Object> r = new LinkedHashMap<>();
    r.put("protocol", Protocol.VERSION);
    r.put("sdk", SdkInfo.PACKAGE);
    r.put("sdk_version", info.version);
    Map<String, Object> components = new LinkedHashMap<>(info.components);
    if (coverage != null) {
      components.put("org.jacoco:org.jacoco.core", Coverage.version());
    }
    r.put("components", components);
    r.put("ops", new ArrayList<Object>(supported()));
    // decode(stream, maxDepth) takes the depth, XdrDataInputStream.setMaxInputLen the length
    r.put("decode_limits", "honoured");
    return r;
  }

  private XdrElement decodeRequest(Request req, Class<?> type) {
    byte[] data = req.bytes("xdr");
    int depth = req.integer("depth_limit", 0);
    int len = req.integer("len_limit", 0);
    return Types.decode(type, data, depth, len);
  }

  /** an xdr input of an op whose request may carry no limits. */
  private static XdrElement decodeMember(Request req, String name, Class<?> type) {
    byte[] data = req.bytes(name);
    int depth = req.has("depth_limit") ? req.integer("depth_limit", 0) : DEFAULT_DEPTH;
    int len = req.has("len_limit") ? req.integer("len_limit", 0) : data.length;
    return Types.decode(type, data, depth, len);
  }

  private static Map<String, Object> one(String name, Object value) {
    Map<String, Object> r = new LinkedHashMap<>();
    r.put(name, value);
    return r;
  }

  private static String b64(byte[] b) {
    return Base64.getEncoder().encodeToString(b);
  }

  private Map<String, Object> decodeEncode(Request req) {
    XdrElement value = decodeRequest(req, types.type(req.str("type")));
    return one("xdr", b64(Types.encode(value)));
  }

  private Map<String, Object> decodeJson(Request req) {
    XdrElement value = decodeRequest(req, types.type(req.str("type")));
    String json = Sdk.call(value.getClass().getSimpleName() + ".toJson", value::toJson);
    try {
      return one("json", Json.parse(json));
    } catch (Json.ParseError e) {
      // the sdk wrote something that is not json
      throw new SdkFailure(value.getClass().getSimpleName() + ".toJson", e, true);
    }
  }

  private Map<String, Object> encodeJson(Request req) {
    Class<?> type = types.type(req.str("type"));
    String json = Json.write(req.raw("json"));
    Method fromJson = Types.method(type, "fromJson", String.class);
    XdrElement value =
        (XdrElement) Sdk.call(type.getSimpleName() + ".fromJson", () -> fromJson.invoke(null, json));
    return one("xdr", b64(Types.encode(value)));
  }

  /**
   * text, or text_base64 for input that is not utf-8. the StrKey decoders take a String (a
   * char[] for seeds), which cannot hold bytes that are not utf-8 without a lossy mapping.
   */
  private static String strkeyText(Request req) {
    boolean text = req.has("text");
    boolean bytes = req.has("text_base64");
    if (text == bytes) {
      throw new BadRequest("strkey_decode needs exactly one of text and text_base64");
    }
    if (text) {
      return req.str("text");
    }
    byte[] raw = req.bytes("text_base64");
    try {
      return StandardCharsets.UTF_8
          .newDecoder()
          .onMalformedInput(CodingErrorAction.REPORT)
          .onUnmappableCharacter(CodingErrorAction.REPORT)
          .decode(ByteBuffer.wrap(raw))
          .toString();
    } catch (CharacterCodingException e) {
      throw new Unsupported(
          "StrKey decodes java strings and char arrays, which cannot hold bytes that are not"
              + " utf-8");
    }
  }

  private static Method strkeyMethod(String name, Class<?> param) {
    try {
      return StrKey.class.getMethod(name, param);
    } catch (NoSuchMethodException e) {
      return null;
    }
  }

  private Map<String, Object> strkeyDecode(Request req) {
    String text = strkeyText(req);
    Kind kind = null;
    for (Kind k : KINDS) {
      if (!text.isEmpty() && text.charAt(0) == k.prefix()) {
        kind = k;
      }
    }
    if (kind == null) {
      throw new SdkFailure(
          "no StrKey decoder for a string starting with "
              + (text.isEmpty() ? "nothing" : Json.write(text.substring(0, 1))));
    }
    boolean seed = kind.kind().equals("seed");
    Method decode = strkeyMethod("decode" + kind.codec(), seed ? char[].class : String.class);
    if (decode == null) {
      // the protocol counts a missing decoder for a prefix as the sdk refusing the string
      throw new SdkFailure(
          "StrKey.decode" + kind.codec() + " does not exist in " + SdkInfo.PACKAGE + " " + info.version);
    }
    Object arg = seed ? text.toCharArray() : text;
    byte[] payload =
        (byte[]) Sdk.call("StrKey.decode" + kind.codec(), () -> decode.invoke(null, arg));
    Map<String, Object> r = new LinkedHashMap<>();
    r.put("kind", kind.kind());
    r.put("payload", b64(payload));
    return r;
  }

  private Map<String, Object> strkeyEncode(Request req) {
    String name = req.str("kind");
    byte[] payload = req.bytes("payload");
    Kind kind = null;
    for (Kind k : KINDS) {
      if (k.kind().equals(name)) {
        kind = k;
      }
    }
    if (kind == null) {
      throw new BadRequest("unknown strkey kind " + name);
    }
    Method encode = strkeyMethod("encode" + kind.codec(), byte[].class);
    if (encode == null) {
      throw new Unsupported(
          SdkInfo.PACKAGE + " " + info.version + " has no StrKey.encode" + kind.codec());
    }
    Object out = Sdk.call("StrKey.encode" + kind.codec(), () -> encode.invoke(null, (Object) payload));
    // encodeEd25519SecretSeed returns char[], the others String
    String text = out instanceof char[] chars ? new String(chars) : (String) out;
    return one("text", text);
  }

  private static org.stellar.sdk.Asset asset(Request req, String name) {
    Asset xdr = (Asset) decodeMember(req, name, Asset.class);
    return Sdk.call("Asset.fromXdr", () -> org.stellar.sdk.Asset.fromXdr(xdr));
  }

  private Map<String, Object> assetOrder(Request req) {
    org.stellar.sdk.Asset a = asset(req, "a");
    org.stellar.sdk.Asset b = asset(req, "b");
    int order = Sdk.call("Asset.compareTo", () -> a.compareTo(b));
    return one("order", Integer.signum(order));
  }

  /**
   * LiquidityPool.fromXdr is the sdk's one public path that takes the fee: the public
   * constructor always uses LiquidityPool.FEE. both refuse assets not in the sdk's order.
   */
  private Map<String, Object> poolId(Request req) {
    int fee = req.integer("fee", Integer.MIN_VALUE);
    Asset a = (Asset) decodeMember(req, "a", Asset.class);
    Asset b = (Asset) decodeMember(req, "b", Asset.class);
    String id =
        Sdk.call(
            "LiquidityPool.getLiquidityPoolId",
            () -> {
              LiquidityPoolParameters params = new LiquidityPoolParameters();
              params.setDiscriminant(LiquidityPoolType.LIQUIDITY_POOL_CONSTANT_PRODUCT);
              LiquidityPoolConstantProductParameters cp = new LiquidityPoolConstantProductParameters();
              cp.setAssetA(a);
              cp.setAssetB(b);
              cp.setFee(new Int32(fee));
              params.setConstantProduct(cp);
              return LiquidityPool.fromXdr(params).getLiquidityPoolId();
            });
    return one("pool_id", id);
  }

  /**
   * Scv.toMap takes a java Map, so the entries go in as a LinkedHashMap in input order. a Map
   * holds one entry per key: input that repeats a key (by SCVal.equals) has no form the sdk's
   * api can be given.
   */
  private Map<String, Object> mapSort(Request req) {
    if (!(req.raw("entries") instanceof List<?> entries)) {
      throw new BadRequest("map_sort needs array member entries");
    }
    Map<SCVal, SCVal> map = new LinkedHashMap<>();
    int i = 0;
    for (Object e : entries) {
      if (!(e instanceof Map<?, ?> m)) {
        throw new BadRequest("entries[" + i + "] is not an object");
      }
      @SuppressWarnings("unchecked")
      Request entry = new Request(req.op, (Map<String, Object>) m);
      SCVal key = (SCVal) decodeMember(entry, "key", SCVal.class);
      SCVal val = (SCVal) decodeMember(entry, "val", SCVal.class);
      if (map.containsKey(key)) {
        throw new Unsupported(
            "Scv.toMap takes a java Map, which holds one entry per key; entries[" + i
                + "] repeats an earlier key");
      }
      map.put(key, val);
      i++;
    }
    SCVal sorted = Sdk.call("Scv.toMap", () -> Scv.toMap(map));
    return one("xdr", b64(Types.encode(sorted)));
  }

  /**
   * id_parse: java-stellar-sdk reads textual ids where it decodes horizon json, with its own
   * gson (GsonSingleton) into its response types: OfferResponse.id (Long) for offer ids and
   * TransactionResponse.accountMuxedId (BigInteger) for muxed ids. the runner's input goes into
   * that field exactly as it arrived, json string or number, and gson reads it the way the sdk
   * reads horizon responses.
   */
  private Map<String, Object> idParse(Request req) {
    String kind = req.str("kind");
    Object input = req.raw("input");
    if (!(input instanceof String || input instanceof Json.Number)) {
      throw new BadRequest("id_parse input must be a string or a number");
    }
    // a number keeps its source digits, a string is written back as the same json string
    String raw = Json.write(input);
    switch (kind) {
      case "offer":
        {
          OfferResponse offer =
              horizonJson("{\"id\":" + raw + "}", OfferResponse.class);
          if (offer.getId() == null) {
            throw new SdkFailure("OfferResponse.id is null after reading " + raw);
          }
          return one("id", Long.toString(offer.getId()));
        }
      case "muxed":
        {
          TransactionResponse tx =
              horizonJson("{\"account_muxed_id\":" + raw + "}", TransactionResponse.class);
          BigInteger id = tx.getAccountMuxedId();
          if (id == null) {
            throw new SdkFailure("TransactionResponse.accountMuxedId is null after reading " + raw);
          }
          // the id the network sees is the one the sdk writes into the M address, which refuses
          // ids outside the uint64 range
          String address =
              Sdk.call("MuxedAccount.getAddress", () -> new MuxedAccount(MUXED_BASE, id).getAddress());
          byte[] payload =
              Sdk.call("StrKey.decodeMed25519PublicKey", () -> StrKey.decodeMed25519PublicKey(address));
          return one("id", new BigInteger(1, java.util.Arrays.copyOfRange(payload, 32, 40)).toString());
        }
      default:
        throw new BadRequest("id_parse kind must be muxed or offer, not " + kind);
    }
  }

  /**
   * GsonSingleton.getInstance().fromJson, the reader Server uses for horizon responses. it is
   * called through reflection because gson, whose version follows the sdk's, is only on the
   * testee's class path.
   */
  private static <T> T horizonJson(String json, Class<T> type) {
    return Sdk.call(
        "GsonSingleton.fromJson into " + type.getSimpleName(),
        () -> {
          Object gson =
              Class.forName("org.stellar.sdk.responses.gson.GsonSingleton")
                  .getMethod("getInstance")
                  .invoke(null);
          Method fromJson = gson.getClass().getMethod("fromJson", String.class, Class.class);
          return type.cast(fromJson.invoke(gson, json, type));
        });
  }

  private AbstractTransaction transaction(Request req, boolean inner) {
    String passphrase = req.str("network_passphrase");
    TransactionEnvelope env =
        (TransactionEnvelope) decodeRequest(req, TransactionEnvelope.class);
    if (inner && env.getDiscriminant() != EnvelopeType.ENVELOPE_TYPE_TX_FEE_BUMP) {
      throw new BadRequest("inner is only defined for a fee bump envelope");
    }
    AbstractTransaction tx =
        Sdk.call(
            "AbstractTransaction.fromEnvelopeXdr",
            () -> AbstractTransaction.fromEnvelopeXdr(env, new Network(passphrase)));
    if (inner) {
      return Sdk.call(
          "FeeBumpTransaction.getInnerTransaction",
          () -> ((FeeBumpTransaction) tx).getInnerTransaction());
    }
    return tx;
  }

  private Map<String, Object> txHash(Request req) {
    boolean inner = req.has("inner") && req.bool("inner");
    AbstractTransaction tx = transaction(req, inner);
    return one("hash", Sdk.call("AbstractTransaction.hashHex", tx::hashHex).toLowerCase());
  }

  private Map<String, Object> balanceId(Request req) {
    int index = req.integer("op_index", 0);
    AbstractTransaction tx = transaction(req, false);
    // the id comes from the transaction that holds the operation, the inner one of a fee bump
    Transaction holder =
        tx instanceof FeeBumpTransaction fb
            ? Sdk.call("FeeBumpTransaction.getInnerTransaction", fb::getInnerTransaction)
            : (Transaction) tx;
    String id = Sdk.call("Transaction.getClaimableBalanceId", () -> holder.getClaimableBalanceId(index));
    return one("balance_id", id.toLowerCase());
  }

  /**
   * Asset.getContractId is the sdk's api for an asset's contract. a preimage from an address
   * has no api of its own, so its id is the sdk's Util.hash of the sdk's encoding of the
   * HashIDPreimage, the way Asset.getContractId builds it.
   */
  private Map<String, Object> contractId(Request req) {
    String passphrase = req.str("network_passphrase");
    ContractIDPreimage preimage =
        (ContractIDPreimage) decodeMember(req, "preimage", ContractIDPreimage.class);
    Network network = new Network(passphrase);
    if (preimage.getDiscriminant() == ContractIDPreimageType.CONTRACT_ID_PREIMAGE_FROM_ASSET) {
      String id =
          Sdk.call(
              "Asset.getContractId",
              () -> org.stellar.sdk.Asset.fromXdr(preimage.getFromAsset()).getContractId(network));
      return one("contract_id", HEX.formatHex(Sdk.call("StrKey.decodeContract", () -> StrKey.decodeContract(id))));
    }
    byte[] id =
        Sdk.call(
            "HashIDPreimage.toXdrByteArray",
            () -> {
              HashIDPreimage full =
                  HashIDPreimage.builder()
                      .discriminant(EnvelopeType.ENVELOPE_TYPE_CONTRACT_ID)
                      .contractID(
                          HashIDPreimage.HashIDPreimageContractID.builder()
                              .networkID(new Hash(network.getNetworkId()))
                              .contractIDPreimage(preimage)
                              .build())
                      .build();
              return Util.hash(full.toXdrByteArray());
            });
    return one("contract_id", HEX.formatHex(id));
  }
}
