package tech.rws8.hitop.core

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi
import java.nio.file.Files

/** One stub server per test JVM; hitop-core's client is built once, after the proxy is set. */
object TestStub {
    val server: StubServer by lazy {
        StubServer().start().also { s -> Ffi.call("useHttpProxy", buildJsonObject { put("proxy", s.proxy) }) }
    }

    fun library(seed: Boolean = true): Triple<Library, LibraryStore, MemorySecretStore> {
        server
        val store = LibraryStore(Files.createTempDirectory("hitop-library").toFile())
        val secrets = MemorySecretStore()
        val lib = Library(store, secrets)
        lib.load()
        if (seed) lib.seedShopApi()
        return Triple(lib, store, secrets)
    }
}
