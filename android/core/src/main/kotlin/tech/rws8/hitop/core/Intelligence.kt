package tech.rws8.hitop.core

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

/*
 * On-device AI (the iOS app's Intelligence.swift rules; Gemini Nano through ML Kit on Android):
 * what the model may see, the prompts, checks it suggests (validated against the actual response
 * and rendered as pm.test), requests from text. Nothing leaves the phone.
 */

/** What the model may see of a request and its response: secrets and credentials masked, bodies cut. */
object ModelInput {
    const val RESPONSE_CHARS = 2400
    const val REQUEST_CHARS = 600
    const val STRING_CHARS = 160

    private val credentialHeaders = Regex("^(authorization|proxy-authorization|cookie|set-cookie|x-api-key|api-key|x-auth-token|x-access-token|x-csrf-token|.*token.*|.*secret.*|.*session.*|.*password.*|.*apikey.*)$", RegexOption.IGNORE_CASE)
    private val credentialKeys = Regex("^(password|passwd|pass|secret|client_secret|access_token|refresh_token|id_token|token|api_key|apikey|key|auth|authorization|session|sessionid|cookie|otp|pin|private_key|signature)$", RegexOption.IGNORE_CASE)

    fun mask(text: String, secrets: kotlin.collections.Collection<String>): String =
        secrets.filter { it.length >= 3 }.sortedByDescending { it.length }.fold(text) { t, s -> t.replace(s, "••••") }

    fun header(name: String, value: String, secrets: kotlin.collections.Collection<String>): String = when {
        name.equals("authorization", true) || name.equals("proxy-authorization", true) -> value.substringBefore(' ', "").let { if (it.isEmpty() || it == value) "••••" else "$it ••••" }
        credentialHeaders.matches(name) -> "••••"
        else -> mask(value, secrets)
    }

    fun url(url: String, secrets: kotlin.collections.Collection<String>): String {
        val masked = mask(url, secrets)
        val q = masked.substringAfter('?', "")
        if (q.isEmpty()) return masked
        val params = q.split('&').joinToString("&") { p -> val k = p.substringBefore('='); if (credentialKeys.matches(k) && p.contains('=')) "$k=••••" else p }
        return masked.substringBefore('?') + "?" + params
    }

    /** A body for the model: JSON with credential keys masked, long strings and arrays cut; text cut at [max]. */
    fun body(text: String, max: Int, secrets: kotlin.collections.Collection<String>): String {
        val t = mask(text, secrets)
        val json = runCatching { HitopJson.parseToJsonElement(t) }.getOrNull()
        val out = if (json != null && (json is JsonObject || json is JsonArray)) PrettyJson.encodeToString(JsonElement.serializer(), shrink(json)) else t
        return if (out.length > max) out.substring(0, max) + "\n… (${out.length - max} more characters)" else out
    }

    private fun shrink(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.mapValues { (k, v) -> if (credentialKeys.matches(k) && v !is JsonObject && v !is JsonArray) JsonPrimitive("••••") else shrink(v) })
        is JsonArray -> if (e.size > 3) JsonArray(e.take(3).map(::shrink) + JsonPrimitive("… ${e.size - 3} more")) else JsonArray(e.map(::shrink))
        is JsonPrimitive -> if (e.isString && e.content.length > STRING_CHARS) JsonPrimitive(e.content.take(STRING_CHARS) + "…") else e
        else -> e
    }

    /** The JSON's shape two levels deep ("{id: string, items: [object], total: {amount: number}}"). */
    fun shape(text: String): String? {
        val json = runCatching { HitopJson.parseToJsonElement(text) }.getOrNull() ?: return null
        fun kind(e: JsonElement, depth: Int): String = when (e) {
            is JsonObject -> if (depth >= 2) "object" else e.entries.joinToString(", ", "{", "}") { (k, v) -> "$k: ${kind(v, depth + 1)}" }
            is JsonArray -> "[" + (e.firstOrNull()?.let { kind(it, depth + 1) } ?: "") + "]"
            is JsonNull -> "null"
            is JsonPrimitive -> if (e.isString) "string" else if (e.content == "true" || e.content == "false") "boolean" else "number"
        }
        return kind(json, 0)
    }

    fun auth(a: Auth?): String = when (a?.type) { null, "none" -> "none"; else -> "${a.type} (value hidden)" }

    /** The model's view of one exchange. */
    fun exchange(r: Request, collectionAuth: Auth?, res: SendResult, secrets: kotlin.collections.Collection<String>): String = buildString {
        append("Request: ${res.method.ifEmpty { r.method }} ${url(res.url.ifEmpty { r.url }, secrets)}\n")
        append("Auth: ${auth(r.auth ?: collectionAuth)}\n")
        res.sentHeaders.ifEmpty { r.headers.filter { it.enabled }.map { it.name to it.value } }.forEach { (k, v) -> append("> $k: ${header(k, v, secrets)}\n") }
        if (res.sentBody.isNotBlank()) append("Request body:\n${body(res.sentBody, REQUEST_CHARS, secrets)}\n")
        append("\nResponse: ${res.status ?: "no answer"} ${res.status?.let(::reason).orEmpty()} in ${res.durationMs} ms, ${res.size} bytes\n")
        res.headers.forEach { (k, v) -> append("< $k: ${header(k, v, secrets)}\n") }
        shape(res.body)?.let { append("Shape: $it\n") }
        if (res.body.isNotEmpty()) append("Body:\n${body(res.body, RESPONSE_CHARS, secrets)}\n")
        res.error?.let { append("Error: ${it.headline}\n") }
    }
}

object Prompts {
    fun explain(input: String) = "You explain HTTP API responses to a developer, in two or three plain sentences: what the request did, what the answer means, anything surprising. No markdown.\n\n$input"

    fun suggestTests(input: String) = "Suggest up to 6 checks for this response as a JSON array, nothing else. Each item is one of: " +
        "{\"kind\":\"status\",\"value\":201}, {\"kind\":\"header\",\"name\":\"Content-Type\",\"contains\":\"json\"}, {\"kind\":\"equals\",\"path\":\"items.0.sku\",\"value\":\"TEE-0193\"}, " +
        "{\"kind\":\"exists\",\"path\":\"links.track\"}, {\"kind\":\"type\",\"path\":\"total.amount\",\"type\":\"number\"}, {\"kind\":\"responseTime\",\"below\":1000}, {\"kind\":\"bodyContains\",\"text\":\"ok\"}.\n\n$input"

    fun requestFromText(text: String) = "Write one HTTP request for this description as JSON, nothing else: {\"name\":\"…\",\"method\":\"GET\",\"url\":\"https://…\",\"headers\":[{\"name\":\"…\",\"value\":\"…\"}],\"body\":\"…\"}. Use \${variables} for values you don't know.\n\n$text"
}

/** A check the model suggested. */
data class SuggestedCheck(val kind: String, val path: String? = null, val name: String? = null, val value: JsonElement? = null, val type: String? = null, val contains: String? = null, val below: Long? = null, val text: String? = null) {
    val title: String
        get() = when (kind) {
            "status" -> "Status is ${value?.let { (it as? JsonPrimitive)?.content }}"
            "header" -> "$name contains $contains"
            "equals" -> "$path equals ${value?.let { if (it is JsonPrimitive && !it.isString) it.content else (it as? JsonPrimitive)?.content ?: it.toString() }}"
            "exists" -> "$path is present"
            "type" -> "$path is a $type"
            "responseTime" -> "Responds under ${formatDuration(below ?: 0)}"
            "bodyContains" -> "Body contains \"$text\""
            else -> kind
        }
}

object TestScript {
    /** The model's answer → checks (any text around the JSON array ignored). */
    fun parse(answer: String): List<SuggestedCheck> {
        val start = answer.indexOf('[')
        val end = answer.lastIndexOf(']')
        if (start < 0 || end <= start) return emptyList()
        val arr = runCatching { HitopJson.parseToJsonElement(answer.substring(start, end + 1)) as JsonArray }.getOrNull() ?: return emptyList()
        return arr.mapNotNull { it as? JsonObject }.map { o ->
            SuggestedCheck(o.str("kind"), o.strOrNull("path"), o.strOrNull("name"), o["value"], o.strOrNull("type"), o.strOrNull("contains"), o.strOrNull("below")?.toDoubleOrNull()?.toLong(), o.strOrNull("text"))
        }
    }

    private fun at(root: JsonElement?, path: String): JsonElement? = path.split('.').filter { it.isNotEmpty() }.fold(root) { e, k ->
        when (e) { is JsonObject -> e[k]; is JsonArray -> k.toIntOrNull()?.let { e.getOrNull(it) }; else -> null }
    }

    private fun typeOf(e: JsonElement?): String? = when (e) {
        null -> null; is JsonNull -> "null"; is JsonObject -> "object"; is JsonArray -> "array"
        is JsonPrimitive -> if (e.isString) "string" else if (e.content == "true" || e.content == "false") "boolean" else "number"
    }

    /** Only checks the actual response passes, at most 6, no duplicates; a time limit needs 2× headroom. */
    fun validate(checks: List<SuggestedCheck>, res: SendResult): List<SuggestedCheck> {
        val root = runCatching { HitopJson.parseToJsonElement(res.body) }.getOrNull()
        return checks.filter { c ->
            when (c.kind) {
                "status" -> (c.value as? JsonPrimitive)?.content?.toIntOrNull() == res.status
                "header" -> c.name != null && c.contains != null && res.headers.any { it.first.equals(c.name, true) && it.second.contains(c.contains, true) }
                "equals" -> c.path != null && c.value != null && at(root, c.path) == c.value
                "exists" -> c.path != null && at(root, c.path).let { it != null && it !is JsonNull }
                "type" -> c.path != null && typeOf(at(root, c.path)) == c.type
                "responseTime" -> c.below != null && res.durationMs * 2 <= c.below
                "bodyContains" -> !c.text.isNullOrEmpty() && res.body.contains(c.text)
                else -> false
            }
        }.distinctBy { it.title }.take(6)
    }

    private fun js(path: String) = path.split('.').joinToString("") { k -> k.toIntOrNull()?.let { "[$it]" } ?: "[${JsonPrimitive(k)}]" }

    /** The checks as a Tests script (pm.test with pm.expect, as the shim runs them). */
    fun render(checks: List<SuggestedCheck>): String = buildString {
        if (checks.any { it.kind in listOf("equals", "exists", "type") }) append("const body = pm.response.json();\n")
        checks.forEach { c ->
            val name = JsonPrimitive(c.title).toString()
            val line = when (c.kind) {
                "status" -> "pm.response.to.have.status(${(c.value as JsonPrimitive).content})"
                "header" -> "pm.expect(pm.response.headers.get(${JsonPrimitive(c.name!!)})).to.include(${JsonPrimitive(c.contains!!)})"
                "equals" -> "pm.expect(body${js(c.path!!)}).to.eql(${c.value})"
                "exists" -> "pm.expect(body${js(c.path!!)}).to.exist"
                "type" -> "pm.expect(body${js(c.path!!)}).to.be.a(${JsonPrimitive(c.type!!)})"
                "responseTime" -> "pm.expect(pm.response.responseTime).to.be.below(${c.below})"
                "bodyContains" -> "pm.expect(pm.response.text()).to.include(${JsonPrimitive(c.text!!)})"
                else -> return@forEach
            }
            append("pm.test($name, () => $line);\n")
        }
    }
}

/** A request the model drafted from text. */
object RequestDraft {
    private val methods = setOf("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")

    fun parse(answer: String): Request? {
        val start = answer.indexOf('{')
        val end = answer.lastIndexOf('}')
        if (start < 0 || end <= start) return null
        val o = runCatching { HitopJson.parseToJsonElement(answer.substring(start, end + 1)).jsonObject }.getOrNull() ?: return null
        val url = o.str("url").trim()
        if (url.isEmpty()) return null
        val method = o.str("method", "GET").uppercase().takeIf { it in methods } ?: "GET"
        val headers = (o["headers"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonObject)?.let { h -> Header(h.str("name"), h.str("value")) } }
            .filter { it.name.matches(Regex("[A-Za-z0-9!#$%&'*+.^_`|~-]+")) }
        var body = o.str("body")
        runCatching { HitopJson.parseToJsonElement(body) }.getOrNull()?.let { body = PrettyJson.encodeToString(JsonElement.serializer(), it) }
        return Request.new(o.str("name").ifBlank { AutoName.of(Request.new(url = url, method = method)) }, url, method).withHeaders(headers).withBody(body)
            .let { if (body.isNotBlank() && body.trimStart().startsWith("{")) it.withBodyKind(BodyKind.JSON) else it }
    }
}

/** The on-device model (Gemini Nano through ML Kit on phones that have it). */
interface LanguageAssistant {
    sealed interface Availability {
        data object Ready : Availability
        data class Unavailable(val reason: String) : Availability
    }

    suspend fun availability(): Availability
    fun stream(prompt: String): Flow<String>
    suspend fun answer(prompt: String): String
}

/** curl → request without the model (the web app's src/curl.js rules). */
object CurlImport {
    data class Result(val request: Request, val ignored: List<String>)

    fun looksLikeCurl(text: String) = text.trimStart().startsWith("curl ")

    private fun words(text: String): List<String> {
        val t = text.replace("\\\n", " ").replace('“', '"').replace('”', '"').replace('‘', '\'').replace('’', '\'').replace('—', '-').replace('–', '-')
        val out = ArrayList<String>()
        val cur = StringBuilder()
        var quote: Char? = null
        var any = false
        var i = 0
        while (i < t.length) {
            val c = t[i]
            when {
                quote == '\'' -> if (c == '\'') quote = null else cur.append(c)
                quote == '"' -> when (c) { '"' -> quote = null; '\\' -> { if (i + 1 < t.length) { cur.append(t[i + 1]); i++ } }; else -> cur.append(c) }
                c == '\'' || c == '"' -> { quote = c; any = true }
                c == '\\' && i + 1 < t.length -> { cur.append(t[i + 1]); i++ }
                c.isWhitespace() -> { if (cur.isNotEmpty() || any) out.add(cur.toString()); cur.clear(); any = false }
                else -> cur.append(c)
            }
            i++
        }
        if (cur.isNotEmpty() || any) out.add(cur.toString())
        return out
    }

    fun parse(text: String): Result? {
        val w = words(text.trim())
        if (w.firstOrNull() != "curl") return null
        var method: String? = null
        var url: String? = null
        val headers = ArrayList<Header>()
        val data = ArrayList<String>()
        val form = ArrayList<FormRow>()
        var user: String? = null
        var get = false
        val ignored = ArrayList<String>()
        var i = 1
        fun next(): String? = w.getOrNull(++i)
        while (i < w.size) {
            val a = w[i]
            when {
                a == "-X" || a == "--request" -> method = next()?.uppercase()
                a.startsWith("-X") && a.length > 2 -> method = a.substring(2).uppercase()
                a == "-H" || a == "--header" -> next()?.let { h -> h.indexOf(':').takeIf { it > 0 }?.let { headers.add(Header(h.substring(0, it).trim(), h.substring(it + 1).trim())) } }
                a in listOf("-d", "--data", "--data-raw", "--data-binary", "--data-ascii") -> next()?.let { data.add(it) }
                a == "--data-urlencode" -> next()?.let { data.add(it) }
                a == "--json" -> next()?.let { data.add(it); headers.add(Header("Content-Type", "application/json")); headers.add(Header("Accept", "application/json")) }
                a == "--form-string" -> next()?.let { f -> form.add(FormRow(f.substringBefore('='), f.substringAfter('=', ""))) }
                a == "-F" || a == "--form" -> next()?.let { f -> form.add(FormRow(f.substringBefore('='), f.substringAfter('=', ""), if (f.substringAfter('=').startsWith("@")) "file" else "text")) }
                a == "-u" || a == "--user" -> user = next()
                a == "-G" || a == "--get" -> get = true
                a == "--url" -> url = next()
                a == "-A" || a == "--user-agent" -> next()?.let { headers.add(Header("User-Agent", it)) }
                a == "-b" || a == "--cookie" -> next()?.let { headers.add(Header("Cookie", it)) }
                a == "-e" || a == "--referer" -> next()?.let { headers.add(Header("Referer", it)) }
                a in listOf("-L", "--location", "-s", "--silent", "-S", "--show-error", "-k", "--insecure", "--compressed", "-v", "--verbose", "-i", "--include", "-f", "--fail") -> if (a in listOf("-k", "--insecure")) ignored.add(a)
                a in listOf("-o", "--output", "--max-time", "-m", "--connect-timeout", "--retry") -> { ignored.add(a); next() }
                a.startsWith("-") -> ignored.add(a)
                url == null -> url = a
            }
            i++
        }
        var u = url ?: return null
        var body = data.joinToString("&")
        if (get && body.isNotEmpty()) { u += (if (u.contains('?')) "&" else "?") + body; body = "" }
        val m = method ?: if (body.isNotEmpty() || form.isNotEmpty()) "POST" else "GET"
        var r = Request.new(url = u, method = m)
        val auth = user?.let { Auth.of("basic").with("username", it.substringBefore(':')).with("password", it.substringAfter(':', "")) }
        if (auth != null) r = r.withAuth(auth)
        if (form.isNotEmpty()) r = r.withBodyKind(BodyKind.MULTIPART).withFormFields(form)
        else if (body.isNotEmpty()) {
            val ct = headers.firstOrNull { it.name.equals("Content-Type", true) }?.value.orEmpty()
            r = if (ct.contains("x-www-form-urlencoded") || (ct.isEmpty() && !body.trimStart().startsWith("{") && !body.trimStart().startsWith("[") && body.contains('=')))
                r.withBodyKind(BodyKind.FORM).withFormFields(body.split('&').map { p -> FormRow(java.net.URLDecoder.decode(p.substringBefore('='), "UTF-8"), java.net.URLDecoder.decode(p.substringAfter('=', ""), "UTF-8")) })
            else r.withBody(body).withBodyKind(if (body.trimStart().startsWith("{") || body.trimStart().startsWith("[")) BodyKind.JSON else BodyKind.TEXT)
        }
        r = r.withHeaders(headers.filterNot { r.bodyKind == BodyKind.FORM && it.name.equals("Content-Type", true) })
        return Result(r.withName(AutoName.of(r)), ignored)
    }
}
