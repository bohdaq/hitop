package tech.rws8.hitop.ffi

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NativeTest {
    @Test
    fun aScriptRunsInHitopCore() {
        val out = Ffi.call("evalScript", buildJsonObject { put("script", "pm.environment.set('who', 'hitop ' + (6 * 7) + ' ✓ 😀')") })
        assertEquals("hitop 42 ✓ 😀", out.jsonObject["variables"]!!.jsonObject["who"]!!.jsonPrimitive.content)
    }

    @Test
    fun syncFfiOperationsAnswerToo() {
        val id = Ffi.call("randomId")
        assertTrue((id as JsonPrimitive).content.matches(Regex("[0-9a-f]{32}")))
    }

    @Test
    fun nulAndEmojiSurviveTheBoundary() {
        val text = "a\u0000b 😀"
        val out = Ffi.call("echo", buildJsonObject { put("text", text) })
        assertEquals(text, out.jsonPrimitive.content)
    }

    @Test
    fun errorsBecomeExceptions() {
        val e = runCatching { Ffi.call("noSuchOperation") }.exceptionOrNull()
        assertTrue(e is FfiException && e.message!!.contains("noSuchOperation"))
    }
}
