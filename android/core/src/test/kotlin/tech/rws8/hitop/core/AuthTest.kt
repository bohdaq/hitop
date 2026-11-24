package tech.rws8.hitop.core

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthTest {
    @Test
    fun clientCredentialsFetchATokenAndUseIt() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val c = lib.state.value.collections.first { it.name == "shop-api" }
        val auth = Auth.of("oauth2").with("tokenUrl", "\${baseUrl}/oauth/token").with("clientId", "app").optional("clientSecret", "s3").optional("scope", "read")
        val r = Request.new(url = "\${baseUrl}/echo?oauth=1").withAuth(auth)
        assertEquals(200, lib.send(c.id, r).status)
        val token = TestStub.server.seen.last { it.url.endsWith("/oauth/token") }
        assertTrue(token.body, token.body.contains("grant_type=client_credentials") && token.body.contains("scope=read"))
        assertEquals("Bearer stub-token", TestStub.server.seen.last { it.url.endsWith("oauth=1") }.header("Authorization"))
    }

    @Test
    fun awsSignsTheRequestAsSent() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val c = lib.state.value.collections.first { it.name == "shop-api" }
        val auth = Auth.of("awsv4").with("accessKey", "AKID").with("secretKey", "shh").with("region", "eu-west-1").with("service", "execute-api")
        assertEquals(200, lib.send(c.id, Request.new(url = "\${baseUrl}/echo?aws=1").withAuth(auth)).status)
        val h = TestStub.server.seen.last { it.url.endsWith("aws=1") }.header("Authorization")!!
        assertTrue(h, h.startsWith("AWS4-HMAC-SHA256 Credential=AKID/") && h.contains("/eu-west-1/execute-api/aws4_request"))
    }

    @Test
    fun digestAnswersTheChallenge() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val c = lib.state.value.collections.first { it.name == "shop-api" }
        val r = Request.new(url = "\${baseUrl}/digest").withAuth(Auth.of("digest").with("username", "ada").with("password", "pw"))
        assertEquals(200, lib.send(c.id, r).status)
        assertTrue(TestStub.server.seen.last { it.url.endsWith("/digest") }.header("Authorization")!!.startsWith("Digest username=\"ada\""))
    }

    @Test
    fun optionalFieldsAreLeftOutWhenEmpty() {
        val a = Auth.of("oauth2").optional("scope", "x").optional("scope", "")
        assertFalse("scope" in a.json)
        assertTrue(a.editable && !a.signsIn)
        assertTrue(a.with("grant", "authorization_code").signsIn)
    }
}
