package org.shapekit.testee

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class TesteeTest {
    private fun ask(line: String): JsonObject = handleLine(line)

    private fun errorClass(r: JsonObject): String? = r["error"]?.jsonObject?.get("class")?.jsonPrimitive?.content

    private fun result(r: JsonObject): JsonObject {
        assertEquals("ok", r["status"]?.jsonPrimitive?.content, r.toString())
        return r["result"]!!.jsonObject
    }

    @Test
    fun helloNamesTheSdkOnTheClasspath() {
        val r = result(ask("""{"id":"1","op":"hello"}"""))
        assertEquals("com.soneso.stellar:stellar-sdk-jvm", r["sdk"]?.jsonPrimitive?.content)
        assertEquals("ignored", r["decode_limits"]?.jsonPrimitive?.content)
        val ops = r["ops"]!!.jsonArray.map { it.jsonPrimitive.content }
        assertTrue("address_from_scval" in ops && "map_validate" !in ops, ops.toString())
    }

    @Test
    fun malformedRequestsAreBadRequests() {
        for (line in listOf(
            "not json",
            """{"op":"hello"}""",
            """{"id":"1","op":"nope"}""",
            // base64 without its padding
            """{"id":"2","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAc","depth_limit":1000,"len_limit":8}""",
            """{"id":"3","op":"id_parse","kind":"offer","input":null}""",
            """{"id":"4","op":"pool_id","a":"AAAAAA==","b":"AAAAAA==","fee":"30"}""",
            """{"id":"5","op":"balance_id","xdr":"AAAAAA==","network_passphrase":"x","op_index":"0"}""",
        )) {
            assertEquals("bad-request", errorClass(ask(line)), line)
        }
    }

    @Test
    fun anScValRoundTripsAndTrailingBytesAreRefused() {
        val ok = ask("""{"id":"1","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAc=","depth_limit":1000,"len_limit":8}""")
        assertEquals("AAAAAwAAAAc=", result(ok)["xdr"]?.jsonPrimitive?.content)
        val trailing = ask("""{"id":"2","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAcA","depth_limit":1000,"len_limit":9}""")
        assertEquals("rejected", errorClass(trailing))
    }

    @Test
    fun aTypeWithoutAClassIsUnsupported() {
        val r = ask("""{"id":"1","op":"decode_encode","type":"NoSuchType","xdr":"AAAAAA==","depth_limit":1000,"len_limit":4}""")
        assertEquals("unsupported", r["status"]?.jsonPrimitive?.content)
    }

    @Test
    fun addressFromScVal() {
        // SCV_ADDRESS of the account with key 32 bytes of 0x3a, then a u32
        val r = ask("""{"id":"1","op":"address_from_scval","xdr":"AAAAEgAAAAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo="}""")
        assertEquals("GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ", result(r)["text"]?.jsonPrimitive?.content)
        assertEquals("rejected", errorClass(ask("""{"id":"2","op":"address_from_scval","xdr":"AAAAAwAAAAc="}""")))
    }

    @Test
    fun idParseTakesNumbersOnly() {
        assertEquals("16", result(ask("""{"id":"1","op":"id_parse","kind":"offer","input":16}"""))["id"]?.jsonPrimitive?.content)
        assertEquals("unsupported", ask("""{"id":"2","op":"id_parse","kind":"offer","input":"16"}""")["status"]?.jsonPrimitive?.content)
    }

    @Test
    fun whatAnSdkCallThrowsIsTheSdksAnythingElseTheTestees() {
        val refused = runCatching { sdk { throw IllegalArgumentException("x") } }.exceptionOrNull() as SdkFailure
        assertEquals("rejected", refused.cls)
        val crash = runCatching { sdk { throw NullPointerException("x") } }.exceptionOrNull() as SdkFailure
        assertEquals("panic", crash.cls)
    }
}
