package tech.rws8.hitop.core

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.rws8.hitop.cloud.CloudCredentials
import tech.rws8.hitop.cloud.CloudSettings
import tech.rws8.hitop.cloud.FakeCloud
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.cloud.MemoryCloudSettingsStore
import java.io.File
import java.nio.file.Files
import java.util.concurrent.CountDownLatch

class ReliabilityTest {
    @Test
    fun aCancelledSendAnswersAtOnceAndKeepsNothing() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val (c, _) = lib.state.value.find("1103")!!
        val history = lib.state.value.history.size
        val latch = CountDownLatch(1)
        TestStub.server.hold = latch
        try {
            val job = async { lib.send(c.id, Request.new(url = "\${baseUrl}/hold")) }
            withTimeout(20_000) { while (TestStub.server.seen.none { it.url.endsWith("/hold") }) delay(10) }
            withTimeout(5_000) { job.cancelAndJoin() }
            assertTrue(job.isCancelled)
            assertEquals(1, latch.count)
            assertEquals(history, lib.state.value.history.size)
        } finally {
            latch.countDown()
        }
        assertEquals(200, lib.send(c.id, lib.state.value.find("1103")!!.second).status)
    }

    @Test
    fun draftsOutliveTheProcessUntilSavedOrDeleted() {
        val dir = Files.createTempDirectory("lib").toFile()
        fun open() = Library(LibraryStore(dir), MemorySecretStore()).also { it.load() }
        val lib = open().also { it.seedShopApi() }
        val (c, r) = lib.state.value.find("1103")!!
        val edited = r.withUrl("https://edited.example/")
        lib.saveDrafts(mapOf(r.id to edited, "gone" to r, "1104" to lib.state.value.find("1104")!!.second))
        assertEquals(mapOf(r.id to edited), open().loadDrafts())
        open().saveRequest(c.id, edited)
        assertTrue(open().loadDrafts().isEmpty())
    }

    @Test
    fun anUnreadableFileIsKeptAndNeverOverwritten() {
        val dir = Files.createTempDirectory("lib").toFile()
        val lib = Library(LibraryStore(dir), MemorySecretStore()).also { it.load(); it.seedShopApi() }
        val broken = "{\"environments\": [ {\"id\""
        File(dir, "environments.json").writeText(broken)
        val store = LibraryStore(dir)
        val again = Library(store, MemorySecretStore()).also { it.load() }
        assertEquals(listOf("environments.json"), again.state.value.damaged)
        // A write before anyone set it aside moves it first.
        again.setVariable(again.addEnvironment("New").id, "a", "b", secret = false)
        val kept = dir.listFiles()!!.single { it.name.startsWith("environments.json.damaged-") }
        assertEquals(broken, kept.readText())
        assertEquals(lib.state.value.collections.size, again.state.value.collections.size)
        assertNotNull(LibraryStore(dir).loadEnvironments().first.single())
        assertTrue(LibraryStore(dir).damaged.isEmpty())
    }

    private class Phone(val dir: File, fake: FakeCloud, val secrets: MemorySecretStore, settings: MemoryCloudSettingsStore) {
        val store = LibraryStore(dir)
        val lib = Library(store, secrets).also { it.load() }
        val cloud = HitopCloud(fake, CloudCredentials(secrets), settings)
        val sync = LibrarySync(cloud, lib, File(dir, "library-sync.json"), CoroutineScope(SupervisorJob() + Dispatchers.Default)).also {
            it.pushDelayMs = 3_600_000
            it.forgetDamaged(store)
            store.setAsideDamaged()
        }
    }

    @Test
    fun aDamagedCollectionComesBackFromTheCloudInsteadOfBeingDeletedThere() = runBlocking {
        val dir = Files.createTempDirectory("lib").toFile()
        val fake = FakeCloud()
        val secrets = MemorySecretStore()
        val settings = MemoryCloudSettingsStore(CloudSettings(server = FakeCloud.SERVER))
        val one = Phone(dir, fake, secrets, settings)
        one.lib.seedShopApi()
        one.cloud.startBrowserSignIn("github", FakeCloud.SERVER)
        one.cloud.finishBrowserSignIn(fake.approve())
        one.sync.sync()
        one.sync.flush()
        assertFalse(one.sync.dirty)
        val c = one.lib.state.value.collections.first { it.name == "shop-api" }
        File(dir, "collections/${c.id}.json").writeText("")
        File(dir, "environments.json").writeText("not json")
        val two = Phone(dir, fake, secrets, settings)
        assertNull(two.lib.state.value.collection(c.id))
        assertTrue(two.lib.state.value.environments.isEmpty())
        two.sync.sync()
        two.sync.flush()
        assertEquals(c.json, two.lib.state.value.collection(c.id)?.json)
        assertEquals(one.lib.state.value.environments.map { it.name }, two.lib.state.value.environments.map { it.name })
        // Still in the cloud, for a phone that starts empty.
        val other = Phone(Files.createTempDirectory("lib").toFile(), fake, secrets, settings)
        other.sync.sync()
        assertNotNull(other.lib.state.value.collection(c.id))
        assertEquals(2, dir.walkTopDown().count { it.name.contains(".damaged-") })
    }
}
