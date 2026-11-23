package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/*
 * The web app's JSON model (shared/services, hitop-core model.rs), kept as the JSON it was read
 * from: every type below wraps its JsonObject and reads typed views of it, and every change
 * writes only the keys it changes. So keys this app doesn't model (oauth2/awsv4/digest auth,
 * examples, openapi records, settings, ...) and fields of an unexpected type survive any edit.
 */

/** Request ids: milliseconds since 1970 with a counter, as the web app's `generateId` makes them. */
object Ids {
    private var last = 0L

    @Synchronized
    fun next(): Long {
        val now = System.currentTimeMillis()
        last = if (now > last) now else last + 1
        return last
    }
}

/** A header row: `{name, value, enabled}` (older data: `key`). */
data class Header(val name: String, val value: String, val enabled: Boolean = true, val extra: JsonObject = EmptyObject) {
    fun toJson(): JsonObject = JsonObject(LinkedHashMap(extra).apply {
        remove("key")
        put("name", JsonPrimitive(name))
        put("value", JsonPrimitive(value))
        put("enabled", JsonPrimitive(enabled))
    })

    companion object {
        fun of(e: JsonElement): Header? {
            val o = e as? JsonObject ?: return null
            return Header(o.strOrNull("name") ?: o.str("key"), o.str("value"), o.bool("enabled", true), o.without("name", "key", "value", "enabled"))
        }
    }
}

/** A form row (`formFields`): `{key, value, type: text|file, enabled}`. */
data class FormRow(val key: String, val value: String, val type: String = "text", val enabled: Boolean = true, val extra: JsonObject = EmptyObject) {
    fun toJson(): JsonObject = JsonObject(LinkedHashMap(extra).apply {
        put("key", JsonPrimitive(key))
        put("value", JsonPrimitive(value))
        put("type", JsonPrimitive(type))
        put("enabled", JsonPrimitive(enabled))
    })

    companion object {
        fun of(e: JsonElement): FormRow? {
            val o = e as? JsonObject ?: return null
            return FormRow(o.str("key"), o.str("value"), o.str("type", "text"), o.bool("enabled", true), o.without("key", "value", "type", "enabled"))
        }
    }
}

/** A switched-off query parameter (`disabledParams`): kept, not sent. */
data class ParamRow(val key: String, val value: String) {
    fun toJson(): JsonObject = buildJsonObject { put("key", key); put("value", value) }
}

/** Auth: `{type, ...}`, any type kept as written (oauth2, awsv4, digest, ...). */
data class Auth(val json: JsonObject) {
    val type: String get() = json.str("type", "none")
    fun field(name: String): String = json.str(name)
    fun with(name: String, value: String): Auth = Auth(json.with(name, value))

    /** Whether this app edits it (the others are kept and shown by name). */
    val editable: Boolean get() = type in EDITABLE

    companion object {
        val EDITABLE = listOf("none", "bearer", "basic", "apikey")
        fun of(type: String): Auth = Auth(buildJsonObject {
            put("type", type)
            when (type) {
                "bearer" -> put("token", "")
                "basic" -> { put("username", ""); put("password", "") }
                "apikey" -> { put("key", ""); put("value", ""); put("in", "header") }
            }
        })
        val NONE = of("none")
    }
}

enum class BodyKind(val label: String) { NONE("None"), JSON("JSON"), TEXT("Raw"), FORM("Form"), MULTIPART("Multipart"), GRAPHQL("GraphQL") }

data class Request(val json: JsonObject) {
    /** The id's JSON text: exact for 64-bit numbers, and string ids of old web app data stay strings. */
    val id: String get() = (json["id"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content ?: ""
    val name: String get() = json.str("name", "Untitled")
    val url: String get() = json.str("url")
    val method: String get() = json.str("method", "GET").uppercase()
    val headers: List<Header> get() = json.arr("headers")?.mapNotNull { Header.of(it) }?.filter { it.name.isNotEmpty() || it.value.isNotEmpty() } ?: emptyList()
    val body: String get() = json.str("body")
    val preRequestScript: String get() = json.str("preRequestScript")
    val postRequestScript: String get() = json.str("postRequestScript")
    val auth: Auth? get() = json.obj("auth")?.let { Auth(it) }
    val bodyMode: String get() = json.str("bodyMode", "raw")
    val bodyType: String? get() = json.strOrNull("bodyType")
    val formFields: List<FormRow> get() = json.arr("formFields")?.mapNotNull { FormRow.of(it) } ?: emptyList()
    val disabledParams: List<ParamRow> get() = json.arr("disabledParams")?.mapNotNull { (it as? JsonObject)?.let { o -> ParamRow(o.str("key"), o.str("value")) } } ?: emptyList()
    val graphqlVariables: String get() = graphqlField("variables")
    val graphqlOperationName: String get() = graphqlField("operationName")
    val file: String? get() = json.strOrNull("file")?.takeIf { it.isNotEmpty() }
    val kind: String? get() = json.strOrNull("kind")?.takeIf { it.isNotEmpty() }
    val examples: JsonArray get() = json.arr("examples") ?: JsonArray(emptyList())

    private fun graphqlField(k: String): String {
        val v = json.obj("graphql")?.get(k) ?: return ""
        return when {
            v is JsonNull -> ""
            v is JsonPrimitive -> v.content
            // The web app may keep the variables as an object: shown as its JSON text.
            else -> PrettyJson.encodeToString(JsonElement.serializer(), v)
        }
    }

    /** The folder the request is in ("" at the top): its `file` without the last part. */
    val folder: String get() = file?.substringBeforeLast('/', "")?.takeIf { file!!.contains('/') } ?: ""

    val isWebSocket: Boolean get() = kind == "websocket"
    val isGrpc: Boolean get() = kind == "grpc"

    /** The tag a row shows: the method, or WS / SSE / GRPC / GQL. */
    val tag: String
        get() = when {
            isWebSocket -> "WS"
            isGrpc -> "GRPC"
            bodyKind == BodyKind.GRAPHQL -> "GQL"
            method == "GET" && headers.any { it.enabled && it.name.equals("Accept", true) && it.value.contains("text/event-stream") } -> "SSE"
            else -> method
        }

    val bodyKind: BodyKind
        get() = when (bodyMode) {
            "urlencoded" -> BodyKind.FORM
            "multipart" -> BodyKind.MULTIPART
            else -> when (bodyType) {
                "graphql" -> BodyKind.GRAPHQL
                "text", "xml" -> BodyKind.TEXT
                "json" -> BodyKind.JSON
                "none" -> BodyKind.NONE
                else -> {
                    val t = body.trimStart()
                    when {
                        body.isEmpty() -> BodyKind.NONE
                        t.startsWith("{") || t.startsWith("[") -> BodyKind.JSON
                        else -> BodyKind.TEXT
                    }
                }
            }
        }

    fun set(key: String, value: JsonElement?): Request = Request(json.with(key, value))
    fun withName(v: String) = set("name", JsonPrimitive(v))
    fun withUrl(v: String) = set("url", JsonPrimitive(v))
    fun withMethod(v: String) = set("method", JsonPrimitive(v.uppercase()))
    fun withBody(v: String) = set("body", JsonPrimitive(v))
    fun withPre(v: String) = set("preRequestScript", JsonPrimitive(v))
    fun withPost(v: String) = set("postRequestScript", JsonPrimitive(v))
    fun withHeaders(rows: List<Header>) = set("headers", JsonArray(rows.map { it.toJson() }))
    fun withAuth(a: Auth?) = set("auth", a?.json)
    fun withFile(v: String?) = set("file", v?.let { JsonPrimitive(it) })
    fun withFormFields(rows: List<FormRow>) = set("formFields", if (rows.isEmpty()) null else JsonArray(rows.map { it.toJson() }))
    fun withDisabledParams(rows: List<ParamRow>) = set("disabledParams", if (rows.isEmpty()) null else JsonArray(rows.map { it.toJson() }))

    fun withGraphql(variables: String, operationName: String): Request {
        val g = LinkedHashMap(json.obj("graphql") ?: EmptyObject)
        if (variables.isEmpty()) g.remove("variables") else g["variables"] = JsonPrimitive(variables)
        if (operationName.isEmpty()) g.remove("operationName") else g["operationName"] = JsonPrimitive(operationName)
        return set("graphql", if (g.isEmpty()) null else JsonObject(g))
    }

    /** Switches the body kind as the web app's body menu does (the text stays). */
    fun withBodyKind(k: BodyKind): Request = when (k) {
        BodyKind.NONE -> set("bodyMode", JsonPrimitive("raw")).set("bodyType", JsonPrimitive("none"))
        BodyKind.JSON -> set("bodyMode", JsonPrimitive("raw")).set("bodyType", JsonPrimitive("json"))
        BodyKind.TEXT -> set("bodyMode", JsonPrimitive("raw")).set("bodyType", JsonPrimitive("text"))
        BodyKind.GRAPHQL -> set("bodyMode", JsonPrimitive("raw")).set("bodyType", JsonPrimitive("graphql"))
        BodyKind.FORM -> set("bodyMode", JsonPrimitive("urlencoded")).set("bodyType", null)
        BodyKind.MULTIPART -> set("bodyMode", JsonPrimitive("multipart")).set("bodyType", null)
    }

    /**
     * What hitop-core reads: the known keys with the types it expects (a mistyped field kept in
     * the file must not stop a send), every other key as it is.
     */
    fun forEngine(): JsonObject {
        val m = LinkedHashMap(json)
        // Numbers as written (exact), string ids as strings (hitop-core derives a number from them).
        m["id"] = json["id"]?.takeIf { it is JsonPrimitive && it !is JsonNull } ?: JsonPrimitive(0)
        m["name"] = JsonPrimitive(name)
        m["url"] = JsonPrimitive(url)
        m["method"] = JsonPrimitive(method)
        m["headers"] = JsonArray(headers.map { it.toJson() })
        m["body"] = JsonPrimitive(body)
        m["preRequestScript"] = JsonPrimitive(preRequestScript)
        m["postRequestScript"] = JsonPrimitive(postRequestScript)
        if (json["auth"] != null && json["auth"] !is JsonObject) m.remove("auth")
        if (json["formFields"] != null) m["formFields"] = JsonArray(formFields.map { it.toJson() })
        if (json["disabledParams"] != null) m["disabledParams"] = JsonArray(disabledParams.map { it.toJson() })
        if (json["bodyType"] != null && json["bodyType"] !is JsonPrimitive) m.remove("bodyType")
        if (json["bodyMode"] != null && json["bodyMode"] !is JsonPrimitive) m.remove("bodyMode")
        if (json["file"] != null && file == null) m.remove("file")
        return JsonObject(m)
    }

    companion object {
        fun new(name: String = "Untitled", url: String = "", method: String = "GET"): Request = Request(buildJsonObject {
            put("id", Ids.next())
            put("name", name)
            put("url", url)
            put("method", method)
            put("headers", JsonArray(emptyList()))
            put("body", "")
            put("preRequestScript", "")
            put("postRequestScript", "")
        })
    }
}

data class Collection(val json: JsonObject) {
    val id: String get() = (json["id"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content ?: ""
    val name: String get() = json.str("name", "Untitled")
    val requests: List<Request> get() = json.arr("requests")?.mapNotNull { (it as? JsonObject)?.let(::Request) } ?: emptyList()
    val variables: Map<String, String> get() = json.stringMap("variables")
    val auth: Auth? get() = json.obj("auth")?.let { Auth(it) }
    val folders: List<String> get() = json.stringList("folders")
    val org: String? get() = json.strOrNull("org")?.takeIf { it.isNotEmpty() }

    fun set(key: String, value: JsonElement?) = Collection(json.with(key, value))
    fun withName(v: String) = set("name", JsonPrimitive(v))
    fun withRequests(list: List<Request>) = set("requests", JsonArray(list.map { it.json }))
    fun withVariables(v: Map<String, String>) = set("variables", v.toJsonObject())
    fun withFolders(v: List<String>) = set("folders", if (v.isEmpty()) null else v.toJsonArray())
    fun withAuth(a: Auth?) = set("auth", a?.json)

    fun request(id: String): Request? = requests.firstOrNull { it.id == id }
    fun replacing(r: Request): Collection = withRequests(requests.map { if (it.id == r.id) r else it })

    /** Every folder: the stored ones and those requests' files name, sorted as stored first. */
    val allFolders: List<String>
        get() {
            val out = LinkedHashSet(folders)
            requests.mapNotNull { r -> r.folder.takeIf { it.isNotEmpty() } }.forEach { f ->
                var p = ""
                f.split('/').forEach { part -> p = if (p.isEmpty()) part else "$p/$part"; out.add(p) }
            }
            return out.toList()
        }

    /** What hitop-core reads (see [Request.forEngine]). */
    fun forEngine(requests: List<Request> = this.requests): JsonObject {
        val m = LinkedHashMap(json)
        if (json["id"] == null || json["id"] is JsonNull) m["id"] = JsonPrimitive(0)
        m["name"] = JsonPrimitive(name)
        m["requests"] = JsonArray(requests.map { it.forEngine() })
        m["variables"] = variables.toJsonObject()
        if (json["auth"] != null && json["auth"] !is JsonObject) m.remove("auth")
        if (json["folders"] != null) m["folders"] = folders.toJsonArray()
        return JsonObject(m)
    }

    companion object {
        fun new(name: String, requests: List<Request> = emptyList(), variables: Map<String, String> = emptyMap()): Collection = Collection(buildJsonObject {
            put("id", Ids.next())
            put("name", name)
            put("requests", JsonArray(requests.map { it.json }))
            put("variables", variables.toJsonObject())
        })
    }
}

/** An environment: `{id, name, variables, secrets: [names]}`; `team: <org>` = a team environment. */
data class Environment(val json: JsonObject) {
    /** Ids are strings in hitop-core; older web app data may have numbers (their text). */
    val id: String get() = (json["id"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content ?: ""
    val name: String get() = json.str("name", "Untitled")
    val variables: Map<String, String> get() = json.stringMap("variables")
    val secrets: List<String> get() = json.stringList("secrets")
    val team: String? get() = json.strOrNull("team")?.takeIf { it.isNotEmpty() }

    fun set(key: String, value: JsonElement?) = Environment(json.with(key, value))
    fun withName(v: String) = set("name", JsonPrimitive(v))
    fun withVariables(v: Map<String, String>) = set("variables", v.toJsonObject())
    fun withSecrets(v: List<String>) = set("secrets", if (v.isEmpty()) null else v.toJsonArray())

    /** What hitop-core reads: secret values left out (they go in separately). */
    fun forEngine(): JsonObject = buildJsonObject {
        put("id", id)
        put("name", name)
        put("variables", variables.filterKeys { it !in secrets }.toJsonObject())
        put("secrets", secrets.toJsonArray())
    }

    companion object {
        fun new(name: String): Environment = Environment(buildJsonObject {
            put("id", "env-${Ids.next()}")
            put("name", name)
            put("variables", EmptyObject)
        })
        /** The globals scope as the web app keeps it (`hitop_globals`): `{variables, secrets}`. */
        const val GLOBALS_ID = "globals"
    }
}

/** The globals scope: `{variables, secrets}`. */
data class Globals(val variables: Map<String, String> = emptyMap(), val secrets: List<String> = emptyList(), val extra: JsonObject = EmptyObject) {
    fun toJson(): JsonObject = JsonObject(LinkedHashMap(extra).apply {
        put("variables", variables.toJsonObject())
        put("secrets", secrets.toJsonArray())
    })

    companion object {
        fun of(o: JsonObject?): Globals = if (o == null) Globals() else Globals(o.stringMap("variables"), o.stringList("secrets"), o.without("variables", "secrets"))
    }
}
