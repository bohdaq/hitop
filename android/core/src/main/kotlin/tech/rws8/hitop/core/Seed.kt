package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * Seeded data. [sample] is what a first visit starts with (the web app's sample collection);
 * [collections] is the drafts' shop-api for UI tests and screenshots (the iOS app's Seed), answered
 * by [StubServer]. Its hosts are http:// so the stub, an HTTP proxy, sees every request.
 */
object Seed {
    const val SAMPLE_BASE_URL = "https://jsonplaceholder.typicode.com"

    fun sample(): Collection {
        fun r(name: String, method: String, url: String, post: String, headers: List<Header> = emptyList(), body: String = "") =
            Request.new(name, url, method).withHeaders(headers).withBody(body).withPost(post)
        return Collection.new(
            AutoName.SAMPLE_NAME,
            listOf(
                r("List posts", "GET", "\${baseUrl}/posts?_limit=3",
                    "pm.test(\"status is 200\", () => pm.response.to.have.status(200));\npm.test(\"three posts\", () => pm.expect(pm.response.json().length).to.eql(3));\n// The next request reads it as \${postId}.\npm.collectionVariables.set(\"postId\", pm.response.json()[0].id);"),
                r("Get the first post", "GET", "\${baseUrl}/posts/\${postId}",
                    "pm.test(\"same post\", () => pm.expect(pm.response.json().id).to.eql(Number(pm.collectionVariables.get(\"postId\"))));"),
                r("Create a post", "POST", "\${baseUrl}/posts", "pm.test(\"created\", () => pm.response.to.have.status(201));",
                    listOf(Header("Content-Type", "application/json")), "{\n  \"title\": \"Hello from HITOP\",\n  \"body\": \"A sample post\",\n  \"userId\": 1\n}"),
            ),
            mapOf("baseUrl" to SAMPLE_BASE_URL),
        )
    }

    val createOrderBody = """
        {
          "customerId": ${'$'}{customerId},
          "currency": "EUR",
          "items": [
            { "sku": "TEE-0193", "qty": 2 },
            { "sku": "CAP-0007", "qty": 1 }
          ],
          "shipping": { "method": "express" }
        }
    """.trimIndent()

    val createOrderTests = """
        const order = pm.response.json();
        pm.test("Status is 201", () => pm.response.to.have.status(201));
        pm.test("Total equals sum of items", () => pm.expect(order.total.amount).to.eql(6480));
        pm.test("Responds under 1 s", () => pm.expect(pm.response.responseTime).to.be.below(1000));
        pm.test("Has a tracking link", () => pm.expect(order.links.track).to.be.ok);
    """.trimIndent()

    private fun r(id: Long, method: String, name: String, url: String, folder: String, body: String = "", headers: List<Header> = emptyList(),
                  tests: String = "", bodyType: String? = null, extra: Map<String, kotlinx.serialization.json.JsonElement> = emptyMap()): Request {
        val slug = name.lowercase().replace(" ", "-")
        var o = buildJsonObject {
            put("id", id)
            put("name", name)
            put("url", url)
            put("method", method)
            put("headers", JsonArray(headers.map { it.toJson() }))
            put("body", body)
            put("preRequestScript", "")
            put("postRequestScript", tests)
            put("bodyMode", "raw")
            (bodyType ?: if (body.isEmpty()) null else "json")?.let { put("bodyType", it) }
            put("file", if (folder.isEmpty()) slug else "$folder/$slug")
        }
        extra.forEach { (k, v) -> o = o.with(k, v) }
        return Request(o)
    }

    private fun collection(id: Long, name: String, requests: List<Request>, variables: Map<String, String> = emptyMap(), auth: Auth? = null, folders: List<String> = emptyList()) =
        Collection(buildJsonObject {
            put("id", id)
            put("name", name)
            put("requests", JsonArray(requests.map { it.json }))
            put("variables", variables.toJsonObject())
            auth?.let { put("auth", it.json) }
            if (folders.isNotEmpty()) put("folders", folders.toJsonArray())
        })

    private val bearer = Auth(buildJsonObject { put("type", "bearer"); put("token", "\${token}") })

    fun collections(): List<Collection> {
        val json = listOf(Header("Content-Type", "application/json"), Header("Accept", "application/json"), Header("Idempotency-Key", "\${\$guid}", enabled = false))
        val catalog = r(1110, "POST", "Catalog query", "\${baseUrl}/graphql", "catalog",
            body = "query Catalog(\$first: Int) {\n  products(first: \$first) {\n    sku name stock rating\n  }\n}", bodyType = "graphql")
            .withGraphql("{ \"first\": 2 }", "")
        val shop = collection(1001, "shop-api", listOf(
            r(1101, "POST", "Log in", "\${baseUrl}/v2/auth/login", "auth", body = "{\n  \"email\": \"ada@example.com\",\n  \"password\": \"\${password}\"\n}", headers = listOf(json[0])),
            r(1102, "POST", "Refresh token", "\${baseUrl}/v2/auth/refresh", "auth"),
            r(1103, "GET", "List orders", "\${baseUrl}/v2/orders?status=open", "orders", tests = "pm.test(\"Status is 200\", () => pm.response.to.have.status(200));"),
            r(1104, "POST", "Create order", "\${baseUrl}/v2/orders?expand=items", "orders", body = createOrderBody, headers = json, tests = createOrderTests),
            r(1105, "GET", "Get order", "\${baseUrl}/v2/orders/\${orderId}", "orders"),
            r(1106, "PATCH", "Update shipping", "\${baseUrl}/v2/orders/\${orderId}/shipping", "orders", body = "{\n  \"method\": \"standard\"\n}", headers = listOf(json[0])),
            r(1107, "GET", "Order events", "ws://api.shop.example/v2/orders/events", "orders", extra = mapOf("kind" to JsonPrimitive("websocket"))),
            r(1108, "DELETE", "Cancel order", "\${baseUrl}/v2/orders/\${orderId}", "orders"),
            r(1109, "GET", "List products", "\${baseUrl}/v2/products?limit=20", "catalog"),
            catalog,
            r(1111, "POST", "Stock levels", "\${baseUrl}/v2/stock/query", "catalog", body = "{\n  \"skus\": [\"TEE-0193\"]\n}", headers = listOf(json[0])),
            r(1112, "POST", "Create customer", "\${baseUrl}/v2/customers", "customers", body = "{\n  \"name\": \"Ada\"\n}", headers = listOf(json[0])),
            r(1113, "GET", "Get customer", "\${baseUrl}/v2/customers/\${customerId}", "customers"),
        ), mapOf("orderId" to "ord_8KQ2mZ41"), bearer, listOf("auth", "orders", "catalog", "customers"))
        val hooks = collection(1002, "webhooks", listOf(
            r(1201, "POST", "Register endpoint", "\${baseUrl}/v2/webhooks", ""),
            r(1202, "GET", "List endpoints", "\${baseUrl}/v2/webhooks", ""),
            r(1203, "POST", "Send test event", "\${baseUrl}/v2/webhooks/test", ""),
            r(1204, "GET", "Delivery log", "\${baseUrl}/v2/webhooks/deliveries", ""),
            r(1205, "PUT", "Rotate secret", "\${baseUrl}/v2/webhooks/secret", ""),
            r(1206, "DELETE", "Remove endpoint", "\${baseUrl}/v2/webhooks/wh_1", ""),
        ))
        val mine = collection(1003, AutoName.MY_REQUESTS, listOf(
            r(1301, "GET", "GET /get", "http://httpbin.shop.example/get", ""),
            r(1302, "POST", "POST /anything", "http://httpbin.shop.example/anything", "", body = "{\n  \"hello\": \"world\"\n}"),
            r(1303, "GET", "GET /status/418", "http://httpbin.shop.example/status/418", ""),
        ))
        return listOf(shop, hooks, mine)
    }

    fun environments(): List<Environment> = listOf(
        env("env-staging", "Staging", mapOf("baseUrl" to "http://staging.shop.example", "customerId" to "42", "token" to "", "password" to ""), listOf("token", "password")),
        env("env-production", "Production", mapOf("baseUrl" to "http://api.shop.example", "customerId" to "1001", "token" to ""), listOf("token")),
        env("env-local", "Local", mapOf("baseUrl" to "http://localhost.shop.example:8080", "customerId" to "1"), emptyList()),
    )

    private fun env(id: String, name: String, vars: Map<String, String>, secrets: List<String>) = Environment(buildJsonObject {
        put("id", id)
        put("name", name)
        put("variables", vars.toJsonObject())
        if (secrets.isNotEmpty()) put("secrets", secrets.toJsonArray())
    })

    val secrets = listOf(Triple("env-staging", "token", "tok_staging_8f2"), Triple("env-staging", "password", "hunter2"), Triple("env-production", "token", "tok_live_5Yq"))

    fun history(cols: List<Collection>, now: Long = 1_794_912_120_000): List<HistoryEntry> {
        val all = cols.flatMap { it.requests }
        fun entry(id: Long, ago: Long, requestId: String, status: Int, dur: Long, size: Long): HistoryEntry {
            val req = all.first { it.id == requestId }
            return HistoryEntry(id, now - ago * 1000, "shop-api", req,
                ResponseSnapshot(status, listOf("Content-Type" to "application/json"), "{\"ok\":true}", false, dur, size, emptyList()))
        }
        return listOf(
            entry(9001, 120, "1104", 201, 142, 412),
            entry(9002, 380, "1103", 200, 74, 1290),
            entry(9003, 900, "1101", 200, 96, 88),
            entry(9004, 3600, "1108", 409, 61, 34),
            entry(9005, 7200, "1106", 200, 88, 120),
        )
    }
}
