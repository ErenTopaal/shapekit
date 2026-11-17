// the kmp testee for the shapekit conformance runner: answers testee protocol requests
// (docs/testee-protocol.md) with the jvm target of com.soneso.stellar:stellar-sdk. one source,
// built once per pinned sdk version from the maven project under versions/.
package org.shapekit.testee

import com.soneso.stellar.sdk.AbstractTransaction
import com.soneso.stellar.sdk.Address
import com.soneso.stellar.sdk.Asset
import com.soneso.stellar.sdk.AssetTypeNative
import com.soneso.stellar.sdk.CreateClaimableBalanceOperation
import com.soneso.stellar.sdk.FeeBumpTransaction
import com.soneso.stellar.sdk.LiquidityPool
import com.soneso.stellar.sdk.ManageSellOfferOperation
import com.soneso.stellar.sdk.MuxedAccount
import com.soneso.stellar.sdk.Network
import com.soneso.stellar.sdk.Price
import com.soneso.stellar.sdk.StrKey
import com.soneso.stellar.sdk.Transaction
import com.soneso.stellar.sdk.Util
import com.soneso.stellar.sdk.scval.Scv
import com.soneso.stellar.sdk.scval.toNative
import com.soneso.stellar.sdk.xdr.AccountIDXdr
import com.soneso.stellar.sdk.xdr.AssetXdr
import com.soneso.stellar.sdk.xdr.ContractIDPreimageXdr
import com.soneso.stellar.sdk.xdr.HashIDPreimageOperationIDXdr
import com.soneso.stellar.sdk.xdr.HashIDPreimageXdr
import com.soneso.stellar.sdk.xdr.Int64Xdr
import com.soneso.stellar.sdk.xdr.PublicKeyXdr
import com.soneso.stellar.sdk.xdr.SCMapEntryXdr
import com.soneso.stellar.sdk.xdr.SCValXdr
import com.soneso.stellar.sdk.xdr.SequenceNumberXdr
import com.soneso.stellar.sdk.xdr.TransactionEnvelopeXdr
import com.soneso.stellar.sdk.xdr.Uint256Xdr
import com.soneso.stellar.sdk.xdr.Uint32Xdr
import com.soneso.stellar.sdk.xdr.XdrReader
import com.soneso.stellar.sdk.xdr.XdrWriter
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.io.BufferedReader
import java.io.InputStreamReader
import java.lang.reflect.InvocationTargetException
import java.math.BigInteger
import java.util.Base64
import kotlin.reflect.KClass
import kotlin.reflect.KFunction
import kotlin.reflect.full.companionObject
import kotlin.reflect.full.companionObjectInstance
import kotlin.reflect.full.memberFunctions

class BadRequest(message: String) : Exception(message)

class Unsupported(message: String) : Exception(message)

/** A refusal the testee reports for the sdk, with the sdk's own words where there are some. */
class Refused(message: String) : Exception(message)

const val PROTOCOL = 2

val OPS = listOf(
    "decode_encode", "decode_json", "encode_json", "strkey_decode", "strkey_encode",
    "asset_order", "pool_id", "map_sort", "id_parse", "to_native", "address_from_scval",
    "tx_hash", "balance_id", "contract_id",
)

fun main() {
    // deep values recurse through the sdk's decoders, so requests run on a thread with a large
    // stack
    val worker = Thread(null, ::serve, "testee", 512L shl 20)
    worker.start()
    worker.join()
}

fun serve() {
    val input = BufferedReader(InputStreamReader(System.`in`, Charsets.UTF_8))
    val out = System.out.bufferedWriter(Charsets.UTF_8)
    while (true) {
        val line = input.readLine() ?: return
        if (line.isEmpty()) continue
        out.write(handleLine(line).toString())
        out.write("\n")
        out.flush()
    }
}

fun error(id: String, cls: String, message: String): JsonObject = buildJsonObject {
    put("id", id)
    put("status", "error")
    put("error", buildJsonObject {
        put("class", cls)
        put("message", message)
    })
}

/**
 * Running out of stack or memory, a null pointer, a failed cast or an index past an array's
 * end is the sdk crashing. Every other exception is the sdk's refusal: it throws
 * IllegalArgumentException and IllegalStateException (require, check) on purpose for bad input.
 */
fun isCrash(e: Throwable): Boolean = e is StackOverflowError || e is OutOfMemoryError ||
    e is NullPointerException || e is ClassCastException || e is IndexOutOfBoundsException

fun handleLine(line: String): JsonObject {
    val req = try {
        Json.parseToJsonElement(line)
    } catch (e: Exception) {
        return error("", "bad-request", "request is not json: ${e.message}")
    }
    val id = ((req as? JsonObject)?.get("id") as? JsonPrimitive)?.takeIf { it.isString }?.content
        ?: return error("", "bad-request", "request is not an object with a string id")
    return try {
        val op = (req["op"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            ?: throw BadRequest("request has no op")
        buildJsonObject {
            put("id", id)
            put("status", "ok")
            put("result", run(op, req))
        }
    } catch (e: BadRequest) {
        error(id, "bad-request", e.message ?: "")
    } catch (e: Unsupported) {
        buildJsonObject {
            put("id", id)
            put("status", "unsupported")
            put("reason", e.message ?: "")
        }
    } catch (e: Refused) {
        error(id, "rejected", e.message ?: "")
    } catch (t: Throwable) {
        val root = if (t is InvocationTargetException && t.cause != null) t.cause!! else t
        val crash = isCrash(root)
        // a crash names where it happened, the frame in the sdk the exception left
        val where = if (crash) {
            root.stackTrace.firstOrNull { it.className.startsWith("com.soneso") }?.let { " at $it" } ?: ""
        } else {
            ""
        }
        error(id, if (crash) "panic" else "rejected", "${root.javaClass.simpleName}: ${root.message}$where")
    }
}

fun run(op: String, req: JsonObject): JsonElement = when (op) {
    "hello" -> buildJsonObject {
        put("protocol", PROTOCOL)
        put("sdk", "com.soneso.stellar:stellar-sdk-jvm")
        put("sdk_version", sdkVersion())
        put("components", buildJsonObject {})
        put("ops", JsonArray(OPS.map { JsonPrimitive(it) }))
        // XdrReader takes no limits
        put("decode_limits", "ignored")
    }
    "decode_encode" -> {
        val t = Types.of(str(req, "type"))
        val v = decode(t, bytes(req, "xdr"))
        buildJsonObject { put("xdr", b64(t.encode(v))) }
    }
    "decode_json" -> {
        val t = Types.of(str(req, "type"))
        val v = decode(t, bytes(req, "xdr"))
        buildJsonObject { put("json", t.toJson(v)) }
    }
    "encode_json" -> {
        val t = Types.of(str(req, "type"))
        val json = req["json"] ?: throw BadRequest("encode_json needs member json")
        buildJsonObject { put("xdr", b64(t.encode(t.fromJson(json)))) }
    }
    "strkey_decode" -> strkeyDecode(req)
    "strkey_encode" -> strkeyEncode(req)
    "asset_order" -> buildJsonObject {
        put("order", Integer.signum(asset(req, "a").compareTo(asset(req, "b"))))
    }
    "pool_id" -> poolId(req)
    "map_sort" -> mapSort(req)
    "id_parse" -> idParse(req)
    "to_native" -> {
        val v = decode(Types.of("ScVal"), bytes(req, "xdr")) as SCValXdr
        val native = v.toNative()
        // the documented refusal: a value with no native form comes back as the SCValXdr itself
        if (native is SCValXdr) throw Refused("toNative returned the SCValXdr itself: no native form")
        buildJsonObject { put("native", describe(native)) }
    }
    "address_from_scval" -> {
        val v = decode(Types.of("ScVal"), bytes(req, "xdr")) as SCValXdr
        buildJsonObject { put("text", Address.fromSCVal(v).toString()) }
    }
    "tx_hash" -> txHash(req)
    "balance_id" -> balanceId(req)
    "contract_id" -> contractId(req)
    "map_validate" -> throw Unsupported(
        "the kmp sdk has no ScMap validation: it orders keys the host way (Scv.toMap, " +
            "compareScValHostOrder in scval/ScValHostOrder.kt) but offers no check of a map it is given",
    )
    "coverage_snapshot", "coverage_new" -> throw Unsupported("the kmp testee has no coverage build")
    else -> throw BadRequest("unknown op $op")
}

/** The version the sdk reports for itself (Util.getSdkVersion). */
fun sdkVersion(): String = Util.getSdkVersion()

fun str(req: JsonObject, name: String): String =
    (req[name] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: throw BadRequest("$name must be a string")

fun bytes(req: JsonObject, name: String): ByteArray = try {
    Base64.getDecoder().decode(str(req, name))
} catch (e: IllegalArgumentException) {
    throw BadRequest("$name is not base64: ${e.message}")
}

fun b64(b: ByteArray): String = Base64.getEncoder().encodeToString(b)

/**
 * Decodes the whole input: bytes left after the value are a refusal, as the protocol requires.
 * XdrReader keeps its position in a private field and exposes no accessor, so the testee reads
 * that field; it only measures, the decode is the sdk's own.
 */
fun decode(t: Types.XdrType, input: ByteArray): Any {
    val reader = XdrReader(input)
    val v = t.decodeFrom(reader)
    val left = input.size - readerOffset.getInt(reader)
    if (left != 0) throw Refused("$left trailing bytes after the value")
    return v
}

private val readerOffset = XdrReader::class.java.getDeclaredField("offset").apply { isAccessible = true }

/** The sdk's xdr classes by the schema's type names, found by reflection. */
object Types {
    class XdrType(val cls: KClass<*>) {
        private val companion = cls.companionObjectInstance
            ?: throw Unsupported("the kmp sdk class ${cls.simpleName} has no companion")
        private val decodeFn = fn(cls.companionObject!!, "decode", XdrReader::class)
        private val fromJsonFn = fn(cls.companionObject!!, "fromXdrJsonElement", JsonElement::class)
        private val encodeFn = fn(cls, "encode", XdrWriter::class)
        private val toJsonFn = cls.memberFunctions.firstOrNull { it.name == "toXdrJsonElement" && it.parameters.size == 1 }
            ?: throw Unsupported("the kmp sdk class ${cls.simpleName} has no toXdrJsonElement")

        fun decodeFrom(r: XdrReader): Any = decodeFn.call(companion, r)!!
        fun encode(v: Any): ByteArray {
            val w = XdrWriter()
            encodeFn.call(v, w)
            return w.toByteArray()
        }
        fun toJson(v: Any): JsonElement = toJsonFn.call(v) as JsonElement
        fun fromJson(j: JsonElement): Any = fromJsonFn.call(companion, j)!!

        private fun fn(owner: KClass<*>, name: String, arg: KClass<*>): KFunction<*> =
            owner.memberFunctions.firstOrNull { f ->
                f.name == name && f.parameters.size == 2 && f.parameters[1].type.classifier == arg
            } ?: throw Unsupported("the kmp sdk class ${cls.simpleName} has no $name(${arg.simpleName})")
    }

    /** The generated table (TypesGen.kt) for the sdk version on the classpath. */
    private val names: Map<String, String> by lazy {
        val only = TypesGen.ONLY[Util.getSdkVersion()]
            ?: throw IllegalStateException(
                "TypesGen has no table for stellar-sdk-jvm ${Util.getSdkVersion()}; run testees/kmp/gentypes.py"
            )
        TypesGen.COMMON + only
    }

    private val cache = HashMap<String, XdrType>()

    fun of(name: String): XdrType {
        cache[name]?.let { return it }
        val simple = names[name] ?: throw Unsupported(
            "the testee's type table, generated from the xdr schema against stellar-sdk-jvm " +
                "${Util.getSdkVersion()}, has no $name"
        )
        val t = XdrType(Class.forName("com.soneso.stellar.sdk.xdr.$simple").kotlin)
        cache[name] = t
        return t
    }
}

fun kind(k: String, payload: ByteArray): JsonObject = buildJsonObject {
    put("kind", k)
    put("payload", b64(payload))
}

fun strkeyDecode(req: JsonObject): JsonObject {
    val text = when {
        req.containsKey("text") && !req.containsKey("text_base64") -> str(req, "text")
        req.containsKey("text_base64") && !req.containsKey("text") -> {
            val raw = bytes(req, "text_base64")
            val d = Charsets.UTF_8.newDecoder()
            try {
                d.decode(java.nio.ByteBuffer.wrap(raw)).toString()
            } catch (e: java.nio.charset.CharacterCodingException) {
                throw Unsupported("the kmp sdk's strkey decoders take a String; bytes that are not utf-8 have no String form")
            }
        }
        else -> throw BadRequest("strkey_decode needs exactly one of text and text_base64")
    }
    if (text.isEmpty()) throw Refused("empty strkey")
    return when (text[0]) {
        'G' -> kind("account", StrKey.decodeEd25519PublicKey(text))
        'M' -> kind("muxed_account", StrKey.decodeMed25519PublicKey(text))
        'S' -> kind("seed", StrKey.decodeEd25519SecretSeed(text.toCharArray()))
        'T' -> kind("pre_auth_tx", StrKey.decodePreAuthTx(text))
        'X' -> kind("hash_x", StrKey.decodeSha256Hash(text))
        'P' -> kind("signed_payload", StrKey.decodeSignedPayload(text))
        'C' -> kind("contract", StrKey.decodeContract(text))
        'L' -> kind("liquidity_pool", StrKey.decodeLiquidityPool(text))
        'B' -> kind("claimable_balance", StrKey.decodeClaimableBalance(text))
        'W' -> kind("muxed_contract", StrKey.decodeMuxedContract(text))
        else -> throw Refused("no strkey decoder for a string starting with ${text[0]}")
    }
}

fun strkeyEncode(req: JsonObject): JsonObject {
    val p = bytes(req, "payload")
    val text = when (val k = str(req, "kind")) {
        "account" -> StrKey.encodeEd25519PublicKey(p)
        "muxed_account" -> StrKey.encodeMed25519PublicKey(p)
        "seed" -> String(StrKey.encodeEd25519SecretSeed(p))
        "pre_auth_tx" -> StrKey.encodePreAuthTx(p)
        "hash_x" -> StrKey.encodeSha256Hash(p)
        "signed_payload" -> StrKey.encodeSignedPayload(p)
        "contract" -> StrKey.encodeContract(p)
        "liquidity_pool" -> StrKey.encodeLiquidityPool(p)
        "claimable_balance" -> StrKey.encodeClaimableBalance(p)
        "muxed_contract" -> StrKey.encodeMuxedContract(p)
        else -> throw BadRequest("unknown strkey kind $k")
    }
    return buildJsonObject { put("text", text) }
}

fun asset(req: JsonObject, name: String): Asset =
    Asset.fromXdr(decode(Types.of("Asset"), bytes(req, name)) as AssetXdr)

/** LiquidityPool refuses pairs its own order does not put first (LiquidityPool.kt init). */
fun poolId(req: JsonObject): JsonObject {
    val fee = (req["fee"] as? JsonPrimitive)?.content?.toIntOrNull() ?: throw BadRequest("fee must be an integer")
    val pool = LiquidityPool(asset(req, "a"), asset(req, "b"), fee)
    val id = runBlocking { pool.getLiquidityPoolId() }
    return buildJsonObject { put("pool_id", id.lowercase()) }
}

fun mapSort(req: JsonObject): JsonObject {
    val entries = req["entries"] as? JsonArray ?: throw BadRequest("entries must be an array")
    val sc = Types.of("ScVal")
    val list = entries.map { e ->
        val o = e as? JsonObject ?: throw BadRequest("an entry is not an object")
        SCMapEntryXdr(decode(sc, bytes(o, "key")) as SCValXdr, decode(sc, bytes(o, "val")) as SCValXdr)
    }
    return buildJsonObject { put("xdr", b64(sc.encode(Scv.toMap(list)))) }
}

/**
 * ManageSellOfferOperation takes the offer id as a Long and MuxedAccount the muxed id as a
 * ULong, so a json number in range is handed over as that integer; text and numbers outside
 * the range have no integer form.
 */
fun idParse(req: JsonObject): JsonObject {
    val kind = str(req, "kind")
    val input = req["input"] as? JsonPrimitive ?: throw BadRequest("id_parse needs member input")
    if (input.isString) throw Unsupported("the kmp sdk takes $kind ids as numbers only")
    val issuer = "GD7777777777777777777777777777777777777777777777777773DB"
    return when (kind) {
        "offer" -> {
            val n = input.content.toLongOrNull()
                ?: throw Unsupported("ManageSellOfferOperation takes the offer id as a Long; ${input.content} has no Long form")
            val op = ManageSellOfferOperation(AssetTypeNative, Asset.createNonNativeAsset("ABC", issuer), "1", Price(1, 1), n)
            val w = XdrWriter()
            op.toXdr().encode(w)
            val b = w.toByteArray()
            // offerID is the op's last field (ManageSellOfferOp in Stellar-transaction.x)
            buildJsonObject { put("id", java.nio.ByteBuffer.wrap(b, b.size - 8, 8).long.toString()) }
        }
        "muxed" -> {
            val n = input.content.toULongOrNull()
                ?: throw Unsupported("MuxedAccount takes the muxed id as a ULong; ${input.content} has no ULong form")
            val w = XdrWriter()
            MuxedAccount(issuer, n).toXdr().encode(w)
            val b = w.toByteArray()
            // KEY_TYPE_MUXED_ED25519: the type, then the uint64 id, then the key
            buildJsonObject { put("id", java.nio.ByteBuffer.wrap(b, 4, 8).long.toULong().toString()) }
        }
        else -> throw BadRequest("unknown id kind $kind")
    }
}

fun obj(cls: String, text: String): JsonObject = buildJsonObject {
    put("type", "object")
    put("class", cls)
    put("text", text)
}

fun describe(v: Any?): JsonElement = when (v) {
    null -> buildJsonObject { put("type", "null") }
    is Boolean -> buildJsonObject { put("type", "bool"); put("value", v) }
    is Int, is Long, is UInt, is ULong, is BigInteger, is Short, is Byte ->
        buildJsonObject { put("type", "integer"); put("value", v.toString()) }
    is String -> buildJsonObject { put("type", "string"); put("value", v) }
    is ByteArray -> buildJsonObject { put("type", "bytes"); put("value", b64(v)) }
    is Map<*, *> -> buildJsonObject {
        put("type", "map")
        put("entries", buildJsonArray {
            for ((k, x) in v) add(buildJsonObject { put("key", describe(k)); put("value", describe(x)) })
        })
    }
    is List<*> -> buildJsonObject { put("type", "array"); put("items", JsonArray(v.map { describe(it) })) }
    is Address -> obj("Address", v.toString())
    is SCValXdr -> obj("SCValXdr", b64(Types.of("ScVal").encode(v)))
    else -> obj(v.javaClass.simpleName, v.toString())
}

fun network(req: JsonObject) = Network(str(req, "network_passphrase"))

fun transaction(req: JsonObject): AbstractTransaction {
    val env = decode(Types.of("TransactionEnvelope"), bytes(req, "xdr")) as TransactionEnvelopeXdr
    return AbstractTransaction.fromEnvelopeXdr(env, network(req))
}

fun hex(b: ByteArray): String = b.joinToString("") { "%02x".format(it) }

fun txHash(req: JsonObject): JsonObject {
    val tx = transaction(req)
    val inner = req["inner"]
    if (inner != null && (inner !is JsonPrimitive || inner.isString || inner.booleanOrNull == null)) {
        throw BadRequest("inner must be a boolean")
    }
    val hash = if (inner?.jsonPrimitive?.booleanOrNull == true) {
        val fb = tx as? FeeBumpTransaction ?: throw BadRequest("inner on an envelope that is no fee bump")
        runBlocking { fb.innerTransaction.hash() }
    } else {
        runBlocking { tx.hash() }
    }
    return buildJsonObject { put("hash", hex(hash)) }
}

/**
 * The sdk has no balance id function, so the testee hashes the HashIDPreimage of the operation
 * (ENVELOPE_TYPE_OP_ID) the sdk's own encoder writes, from the sdk's transaction object.
 */
fun balanceId(req: JsonObject): JsonObject {
    var tx = transaction(req)
    if (tx is FeeBumpTransaction) tx = tx.innerTransaction
    val t = tx as Transaction
    val index = (req["op_index"] as? JsonPrimitive)?.content?.toIntOrNull()?.takeIf { it >= 0 }
        ?: throw BadRequest("op_index must be a non-negative integer")
    if (index >= t.operations.size) throw Refused("no operation at index $index")
    if (t.operations[index] !is CreateClaimableBalanceOperation) {
        throw Refused("the operation at index $index does not create a claimable balance")
    }
    val source = StrKey.decodeEd25519PublicKey(MuxedAccount(t.sourceAccount).ed25519AccountId)
    val preimage = HashIDPreimageXdr.OperationID(
        HashIDPreimageOperationIDXdr(
            AccountIDXdr(PublicKeyXdr.Ed25519(Uint256Xdr(source))),
            SequenceNumberXdr(Int64Xdr(t.sequenceNumber)),
            Uint32Xdr(index.toUInt()),
        ),
    )
    val w = XdrWriter()
    preimage.encode(w)
    return buildJsonObject { put("balance_id", "00000000" + hex(java.security.MessageDigest.getInstance("SHA-256").digest(w.toByteArray()))) }
}

/** Address.deriveContractId and Asset.getContractId, the sdk's two contract id functions. */
fun contractId(req: JsonObject): JsonObject {
    val preimage = decode(Types.of("ContractIdPreimage"), bytes(req, "preimage")) as ContractIDPreimageXdr
    val net = network(req)
    val id = runBlocking {
        when (preimage) {
            is ContractIDPreimageXdr.FromAddress -> Address.deriveContractId(
                Address.fromSCAddress(preimage.value.address),
                preimage.value.salt.value,
                net,
            )
            is ContractIDPreimageXdr.FromAsset -> Asset.fromXdr(preimage.value).getContractId(net)
        }
    }
    return buildJsonObject { put("contract_id", hex(StrKey.decodeContract(id))) }
}

