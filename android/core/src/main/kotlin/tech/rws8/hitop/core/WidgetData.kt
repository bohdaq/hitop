package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * What the home-screen widgets, the Quick Settings tile and the shortcuts show (the iOS app's
 * WidgetData): names, methods, counts and statuses only, never a response, a variable value, a
 * secret or a host. Written to `widget-snapshot.json` after every run and widget send.
 */
data class WidgetSnapshot(val lastRun: LastRun?, val collections: List<CollectionStatus>, val pins: List<Pin>) {
    data class LastRun(val collectionId: String, val name: String, val environment: String?, val passed: Int, val total: Int, val at: Long)
    data class CollectionStatus(val collectionId: String, val name: String, val ok: Boolean, val passed: Int, val total: Int)
    data class Pin(val requestId: String, val collectionId: String, val method: String, val name: String, val status: Int?)

    fun toJson(): JsonObject = buildJsonObject {
        lastRun?.let { r -> put("lastRun", buildJsonObject { put("collectionId", r.collectionId); put("name", r.name); r.environment?.let { put("environment", it) }; put("passed", r.passed); put("total", r.total); put("at", r.at) }) }
        put("collections", JsonArray(collections.map { c -> buildJsonObject { put("collectionId", c.collectionId); put("name", c.name); put("ok", c.ok); put("passed", c.passed); put("total", c.total) } }))
        put("pins", JsonArray(pins.map { p -> buildJsonObject { put("requestId", p.requestId); put("collectionId", p.collectionId); put("method", p.method); put("name", p.name); p.status?.let { put("status", it) } } }))
    }

    companion object {
        val EMPTY = WidgetSnapshot(null, emptyList(), emptyList())

        fun of(o: JsonObject?): WidgetSnapshot {
            if (o == null) return EMPTY
            val lr = o.obj("lastRun")?.let { WidgetSnapshot.LastRun(it.str("collectionId"), it.str("name"), it.strOrNull("environment"), it.str("passed", "0").toInt(), it.str("total", "0").toInt(), it.str("at", "0").toLong()) }
            val cols = (o["collections"] as? JsonArray).orEmpty().map { it.jsonObject }.map { CollectionStatus(it.str("collectionId"), it.str("name"), it.bool("ok", true), it.str("passed", "0").toInt(), it.str("total", "0").toInt()) }
            val pins = (o["pins"] as? JsonArray).orEmpty().map { it.jsonObject }.map { Pin(it.str("requestId"), it.str("collectionId"), it.str("method"), it.str("name"), it.strOrNull("status")?.toIntOrNull()) }
            return WidgetSnapshot(lr, cols, pins)
        }

        /** The snapshot of the library and the runs as they are now. */
        fun build(s: LibraryState, runs: List<RunRecord>): WidgetSnapshot {
            val last = runs.firstOrNull()?.let { LastRun(it.collectionId, it.collectionName, it.environment, it.passed, it.total, it.startedAt) }
            val latest = runs.groupBy { it.collectionId }.mapValues { it.value.first() }
            val cols = latest.values.sortedByDescending { it.startedAt }.map { CollectionStatus(it.collectionId, it.collectionName, it.ok, it.passed, it.total) }
            val status = s.lastStatus
            val pins = s.pins.mapNotNull { s.find(it) }.map { (c, r) -> Pin(r.id, c.id, r.tag, r.name, status[r.id]) }
            return WidgetSnapshot(last, cols, pins)
        }
    }
}
