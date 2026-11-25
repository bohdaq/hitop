package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

/** An environment to import; its secret values go to the secret store only. */
data class ImportedEnvironment(val environment: Environment, val secretValues: Map<String, String>)

/** What a file holds, read but not yet added. */
data class ImportPlan(
    val kind: String,
    val collections: List<Collection> = emptyList(),
    val environments: List<ImportedEnvironment> = emptyList(),
    val globals: ImportedEnvironment? = null,
    val issues: List<String> = emptyList(),
) {
    val title: String
        get() = when (kind) {
            "postman" -> "Postman collection"
            "openapi" -> "OpenAPI"
            "environment" -> "Postman environment"
            "globals" -> "Postman globals"
            "folder" -> "HITOP collection folder"
            "insomnia" -> "Insomnia"
            "bruno" -> "Bruno collection"
            "har" -> "HAR recording"
            else -> "HITOP collections"
        }
    val isEmpty: Boolean get() = collections.isEmpty() && environments.isEmpty() && globals == null
}

/** Files in and out: hitop-core's importers and exporters (hitop-ffi), zips of collection folders. */
object Importing {
    const val MAX_BYTES = 25 * 1024 * 1024
    private const val MAX_ENTRIES = 5000

    fun isZip(b: ByteArray) = b.size > 3 && b[0] == 'P'.code.toByte() && b[1] == 'K'.code.toByte() && b[2].toInt() == 3 && b[3].toInt() == 4

    /** Blocking. [source]: the address it came from (OpenAPI's relative servers resolve against it). */
    fun read(bytes: ByteArray, name: String = "Imported", source: String? = null): ImportPlan =
        if (isZip(bytes)) unzip(bytes).let { files -> if ("hitop.yaml" in files) folder(files, name.removeSuffix(".zip")) else bruno(files) } else read(String(bytes, Charsets.UTF_8), source)

    fun read(text: String, source: String? = null): ImportPlan {
        val o = Ffi.call("import", buildJsonObject { put("text", text); source?.let { put("source", it) } }).jsonObject
        return plan(o)
    }

    /** A Bruno collection's zip. */
    private fun bruno(files: Map<String, String>): ImportPlan =
        plan(Ffi.call("importFiles", buildJsonObject { put("files", files.filterKeys { it.endsWith(".bru") || it.endsWith("bruno.json") }.toJsonObject()) }).jsonObject)

    private fun plan(o: JsonObject): ImportPlan {
        val env = o.obj("environment")?.let { ImportedEnvironment(Environment(it), o.stringMap("secretValues")) }
        val kind = o.str("kind")
        return ImportPlan(
            kind,
            o.arr("collections").orEmpty().map { Collection(HitopJson.parseToJsonElement(it.jsonPrimitive.content).jsonObject) },
            listOfNotNull(env.takeIf { kind == "environment" }) + o.arr("environments").orEmpty().map { it.jsonObject }.mapNotNull { e -> e.obj("environment")?.let { ImportedEnvironment(Environment(it), e.stringMap("secretValues")) } },
            env.takeIf { kind == "globals" },
            o.stringList("issues"),
        )
    }

    /** A zip's text files by path, a single top folder left out. Bounded: a small file can't fill the memory. */
    fun unzip(bytes: ByteArray): Map<String, String> {
        val out = LinkedHashMap<String, String>()
        var total = 0L
        var entries = 0
        ZipInputStream(ByteArrayInputStream(bytes)).use { zip ->
            while (true) {
                val e = zip.nextEntry ?: break
                require(++entries <= MAX_ENTRIES) { "The zip has too many files." }
                if (e.isDirectory) continue
                val b = ByteArrayOutputStream()
                val buf = ByteArray(8192)
                while (true) {
                    val n = zip.read(buf)
                    if (n < 0) break
                    total += n
                    require(total <= MAX_BYTES) { "The zip is too large." }
                    b.write(buf, 0, n)
                }
                val path = e.name.replace('\\', '/')
                if (path.startsWith("__MACOSX/")) continue
                out[path] = b.toString("UTF-8")
            }
        }
        val marker = out.keys.filter { it == "hitop.yaml" || it.endsWith("/hitop.yaml") }.minByOrNull { it.length } ?: run {
            require(out.keys.any { it == "bruno.json" || it.endsWith("/bruno.json") }) { "The zip has no hitop.yaml or bruno.json: it isn't a collection folder." }
            return out
        }
        val root = marker.removeSuffix("hitop.yaml")
        return out.filterKeys { it.startsWith(root) }.mapKeys { it.key.removePrefix(root) }
    }

    private fun folder(files: Map<String, String>, name: String): ImportPlan {
        val kept = files.filterKeys { (it.endsWith(".yaml") || it.endsWith(".yml")) && (!it.contains('/') || it.startsWith("requests/") || it.startsWith("environments/")) }
        val text = Ffi.call("readFiles", buildJsonObject { put("files", kept.toJsonObject()); put("name", name) }).jsonPrimitive.content
        val o = HitopJson.parseToJsonElement(text).jsonObject
        return ImportPlan(
            "folder",
            listOfNotNull(o.obj("collection")?.let(::Collection)),
            o.arr("environments").orEmpty().map { ImportedEnvironment(Environment(it.jsonObject), emptyMap()) },
            o.obj("globals")?.takeIf { it.stringMap("variables").isNotEmpty() || it.stringList("secrets").isNotEmpty() }?.let { ImportedEnvironment(Environment(it), emptyMap()) },
        )
    }

    /** [c] with ids nothing in [taken] has (an export of something that is still here, imported again). */
    fun fresh(c: Collection, takenCollections: Set<String>, takenRequests: Set<String>): Collection {
        val seen = HashSet(takenRequests)
        val requests = c.requests.map { r -> (if (r.id.isEmpty() || r.id == "0" || r.id in seen) r.set("id", JsonPrimitive(Ids.next())) else r).also { seen.add(it.id) } }
        val out = c.withRequests(requests)
        return if (c.id.isEmpty() || c.id == "0" || c.id in takenCollections) out.set("id", JsonPrimitive(Ids.next())) else out
    }

    /** Without what only means something on this phone (picked files). */
    private fun portable(c: Collection) = c.withRequests(c.requests.map { r -> if (r.formFields.none { "fileToken" in it.extra }) r else r.withFormFields(r.formFields.map { it.copy(extra = it.extra.without("fileToken")) }) })

    fun exportJson(c: Collection, format: String): String =
        Ffi.call("export", buildJsonObject { put("collection", portable(c).forEngine().toString()); put("format", format) }).jsonPrimitive.content

    /** The collection folder (spec/collection-format.md) as a zip. */
    fun exportZip(c: Collection): ByteArray {
        val folder = buildJsonObject { put("collection", portable(c).forEngine()); put("environments", JsonArray(emptyList())) }
        val files = Ffi.call("toFiles", buildJsonObject { put("folder", folder.toString()) }).jsonObject
        val out = ByteArrayOutputStream()
        ZipOutputStream(out).use { z ->
            files.forEach { (p, t) ->
                z.putNextEntry(ZipEntry(p).apply { time = 0 })
                z.write(t.jsonPrimitive.content.toByteArray(Charsets.UTF_8))
                z.closeEntry()
            }
        }
        return out.toByteArray()
    }

    data class SpecSync(val collection: Collection, val added: List<String>, val changed: List<String>, val removed: List<String>, val unchanged: Int, val empty: Boolean)

    fun specSource(c: Collection): String? = c.json.obj("openapi")?.strOrNull("source")?.takeIf { it.startsWith("http://") || it.startsWith("https://") }

    /** [c] synced with the spec in [text]; the requests that are here keep their ids whatever their type. */
    fun syncSpec(c: Collection, text: String, prune: Boolean, source: String? = specSource(c)): SpecSync {
        val ids = c.requests.map { it.json["id"] ?: JsonNull }
        val base = 1_000_000_007L
        val numbered = c.forEngine(c.requests.mapIndexed { i, r -> r.set("id", JsonPrimitive(base + i)) })
        val o = Ffi.call("openapiSync", buildJsonObject { put("collection", JsonObject(numbered + ("id" to JsonPrimitive(1))).toString()); put("text", text); put("prune", prune); source?.let { put("source", it) } }).jsonObject
        val synced = Collection(HitopJson.parseToJsonElement(o.str("collection")).jsonObject)
        val back = synced.withRequests(synced.requests.map { r ->
            val i = (r.id.toLongOrNull() ?: -1) - base
            if (i in ids.indices) r.set("id", ids[i.toInt()]) else r.set("id", JsonPrimitive(Ids.next()))
        }).set("id", c.json["id"])
        val d = o.obj("diff") ?: EmptyObject
        fun names(k: String) = d.arr(k).orEmpty().map { it.jsonObject }.map { it.strOrNull("operation") ?: it.strOrNull("name") ?: it.str("operationId") }
        val changed = d.arr("changed").orEmpty().map { it.jsonObject }.map { e ->
            (e.strOrNull("operation") ?: e.str("name")) + listOfNotNull(
                e.stringList("updated").takeIf { it.isNotEmpty() }?.let { "updates ${it.joinToString()}" },
                e.stringList("kept").takeIf { it.isNotEmpty() }?.let { "keeps your ${it.joinToString()}" },
            ).joinToString("; ", prefix = " · ")
        }
        return SpecSync(back, names("added"), changed, names("removed"), d.str("unchanged", "0").toIntOrNull() ?: 0, o.bool("empty", false))
    }
}
