package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

/** A GraphQL schema from introspection (the web app's graphql-schema.js), for suggestions and browsing. */
data class GraphQLSchema(val types: Map<String, Type>, val query: String?, val mutation: String?, val subscription: String?) {
    data class Field(val name: String, val type: String, val args: List<String>, val description: String?) {
        /** `[Product!]!` → `Product`. */
        val named: String get() = type.trim('[', ']', '!').trimEnd(']', '!')
    }

    data class Type(val name: String, val kind: String, val fields: List<Field>, val enumValues: List<String>, val description: String?)

    /** Fields to offer at [cursor] in [text]: those of the type whose selection set the cursor is in, by the word typed so far. */
    fun suggestions(text: String, cursor: Int): List<Field> {
        val before = text.substring(0, cursor.coerceIn(0, text.length))
        val type = typeAt(before) ?: return emptyList()
        val prefix = Regex("[_A-Za-z][_0-9A-Za-z]*$").find(before)?.value.orEmpty()
        return types[type]?.fields.orEmpty().filter { it.name.startsWith(prefix) && it.name != prefix }.sortedBy { it.name }
    }

    /** The type of the selection set [before] ends in (null outside one). */
    fun typeAt(before: String): String? {
        val stack = ArrayList<String?>()
        var lastWord: String? = null
        var onType: String? = null
        var opRoot: String? = null
        var i = 0
        val s = before
        while (i < s.length) {
            val c = s[i]
            when {
                c == '#' -> {
                    while (i < s.length && s[i] != '\n') i++
                    // The cursor inside a comment.
                    if (i >= s.length) return null
                    continue
                }
                c == '"' -> {
                    i++
                    while (i < s.length && s[i] != '"') { if (s[i] == '\\') i++; i++ }
                    // The cursor inside a string: nothing to suggest.
                    if (i >= s.length) return null
                }
                c == '(' -> {
                    // Arguments: skipped (their values aren't fields).
                    var depth = 1
                    i++
                    while (i < s.length && depth > 0) { if (s[i] == '(') depth++ else if (s[i] == ')') depth--; i++ }
                    continue
                }
                c == '{' -> {
                    val parent = stack.lastOrNull()
                    val next = when {
                        onType != null -> onType
                        stack.isEmpty() -> opRoot ?: query
                        else -> parent?.let { p -> types[p]?.fields?.firstOrNull { it.name == lastWord }?.named }
                    }
                    stack.add(next)
                    onType = null; lastWord = null; opRoot = null
                }
                c == '}' -> { if (stack.isNotEmpty()) stack.removeAt(stack.lastIndex); lastWord = null }
                c.isLetter() || c == '_' -> {
                    val start = i
                    while (i < s.length && (s[i].isLetterOrDigit() || s[i] == '_')) i++
                    val word = s.substring(start, i)
                    if (stack.isEmpty()) {
                        when (word) { "query" -> opRoot = query; "mutation" -> opRoot = mutation; "subscription" -> opRoot = subscription }
                    }
                    if (lastWord == "on") onType = word
                    lastWord = word
                    continue
                }
            }
            i++
        }
        return stack.lastOrNull()
    }

    companion object {
        const val INTROSPECTION = "query IntrospectionQuery { __schema { queryType { name } mutationType { name } subscriptionType { name } types { kind name description fields(includeDeprecated: true) { name description args { name type { ...TypeRef } } type { ...TypeRef } } inputFields { name type { ...TypeRef } } enumValues(includeDeprecated: true) { name } } } }\nfragment TypeRef on __Type { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name } } } } }"

        private fun typeRef(o: JsonObject?): String {
            if (o == null) return "?"
            return when (o.str("kind")) {
                "NON_NULL" -> typeRef(o.obj("ofType")) + "!"
                "LIST" -> "[" + typeRef(o.obj("ofType")) + "]"
                else -> o.str("name")
            }
        }

        /** The schema from an introspection answer; an error worded as the web app words it. */
        fun parse(body: String): GraphQLSchema {
            val root = runCatching { HitopJson.parseToJsonElement(body).jsonObject }.getOrNull() ?: throw IllegalStateException("The answer isn't JSON")
            (root["errors"] as? JsonArray)?.firstOrNull()?.let { e -> throw IllegalStateException((e as? JsonObject)?.str("message") ?: "the server answered with errors") }
            val schema = root.obj("data")?.obj("__schema") ?: throw IllegalStateException("the answer has no __schema (is introspection turned off?)")
            val types = (schema["types"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.associate { t ->
                val fields = ((t["fields"] as? JsonArray) ?: (t["inputFields"] as? JsonArray)).orEmpty().mapNotNull { it as? JsonObject }.map { f ->
                    Field(f.str("name"), typeRef(f.obj("type")), (f["args"] as? JsonArray).orEmpty().mapNotNull { a -> (a as? JsonObject)?.let { "${it.str("name")}: ${typeRef(it.obj("type"))}" } }, f.strOrNull("description"))
                }
                t.str("name") to Type(t.str("name"), t.str("kind"), fields, (t["enumValues"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonObject)?.str("name") }, t.strOrNull("description"))
            }
            fun root(k: String) = schema.obj(k)?.strOrNull("name")
            return GraphQLSchema(types, root("queryType"), root("mutationType"), root("subscriptionType"))
        }

        /** The introspection request: the GraphQL request's URL, headers and auth, POST, no scripts. */
        fun introspectionRequest(r: Request): Request = r.withMethod("POST").withBody(INTROSPECTION).withBodyKind(BodyKind.GRAPHQL).withGraphql("", "IntrospectionQuery").withPre("").withPost("")
    }
}

@Suppress("unused")
private val keep = listOf(JsonNull, JsonPrimitive(""))
