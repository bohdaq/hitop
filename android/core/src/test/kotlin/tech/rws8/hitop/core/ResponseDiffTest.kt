package tech.rws8.hitop.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.rws8.hitop.core.ResponseDiff.Kind

class ResponseDiffTest {
    private fun r(body: String, status: Int = 200, vararg h: Pair<String, String>) = ComparedResponse("x", status, h.toMap(), body)

    @Test
    fun jsonIgnoresKeyOrderAndNamesPaths() {
        assertTrue(ResponseDiff.compare(r("""{"a":1,"b":[1,2]}"""), r("""{"b":[1,2.0],"a":1}""")).same)
        val d = ResponseDiff.compare(r("""{"a":1,"x y":true,"l":[1,2]}"""), r("""{"a":"1","l":[1],"n":null}"""))
        assertEquals(listOf("$.a" to Kind.CHANGED, "$.l[1]" to Kind.REMOVED, "$.n" to Kind.ADDED, "$[\"x y\"]" to Kind.REMOVED), d.body.map { it.path to it.kind })
        assertTrue(d.json)
    }

    @Test
    fun textGoesByLineAndHeadersByName() {
        val d = ResponseDiff.compare(r("a\nb\nc", 200, "etag" to "1", "date" to "x"), r("a\nB\nc\nd", 404, "etag" to "2", "date" to "y"))
        assertEquals(listOf(Kind.SAME, Kind.REMOVED, Kind.ADDED, Kind.SAME, Kind.ADDED), d.body.map { it.kind })
        assertEquals(listOf("etag"), d.headers.map { it.path })
        assertEquals("404", d.status!!.after)
        assertFalse(d.json)
        assertEquals(6, ResponseDiff.lines("a\nb\nc", "d\ne\nf", limit = 1).size)
    }

    @Test
    fun examplesOfAnyShape() {
        val e = ComparedResponse.ofExample("e", HitopJson.parseToJsonElement("""{"status":"201","headers":{"ETag":"1"},"body":{"a":1}}""") as kotlinx.serialization.json.JsonObject)
        assertEquals(201, e.status)
        assertEquals("1", e.headers["etag"])
        assertTrue(ResponseDiff.compare(e, r("""{"a":1}""", 201, "etag" to "1")).same)
    }

    @Test
    fun deletesAndMovesCanBeUndone() {
        val (lib, _, _) = TestStub.library()
        val before = lib.state.value.collections.first { it.name == "shop-api" }
        lib.undoable { lib.deleteRequest("1104") }!!()
        assertEquals(before.requests.map { it.id }, lib.state.value.collection(before.id)!!.requests.map { it.id })
        lib.undoable { lib.moveRequest("1104", before.id, "elsewhere") }!!()
        assertEquals(before.request("1104")!!.file, lib.state.value.collection(before.id)!!.request("1104")!!.file)
        val folder = before.allFolders.first()
        lib.undoable { lib.deleteFolder(before.id, folder) }!!()
        assertEquals(before.json, lib.state.value.collection(before.id)!!.json)
        lib.togglePin("1105")
        val undo = lib.undoable { lib.deleteCollection(before.id) }!!
        assertTrue(lib.state.value.collections.none { it.name == "shop-api" })
        undo()
        val back = lib.state.value.collections.single { it.name == "shop-api" }
        assertEquals(before.requests.map { it.id }, back.requests.map { it.id })
        // HITOP Cloud may have its deletion already: a new collection there, not a conflict.
        assertTrue(back.id != before.id)
        // Nothing changed: nothing to undo.
        assertEquals(null, lib.undoable { })
        // A later change wins over the undo of an older one.
        val c2 = lib.state.value.collections.single { it.name == "shop-api" }
        val u = lib.undoable { lib.deleteRequest("1104") }!!
        lib.renameCollection(c2.id, "renamed")
        u()
        val now = lib.state.value.collection(c2.id)!!
        assertEquals("renamed", now.name)
        assertTrue(now.request("1104") != null)
    }
}
