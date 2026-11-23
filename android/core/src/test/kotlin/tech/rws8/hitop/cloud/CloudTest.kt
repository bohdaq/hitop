package tech.rws8.hitop.cloud

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.rws8.hitop.core.AutoName
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.LibraryStore
import tech.rws8.hitop.core.MemorySecretStore
import java.nio.file.Files

class CloudTest {
    private fun phone(fake: FakeCloud): Triple<Library, HitopCloud, LibrarySync> {
        val secrets = MemorySecretStore()
        val lib = Library(LibraryStore(Files.createTempDirectory("lib").toFile()), secrets).also { it.load() }
        val cloud = HitopCloud(fake, CloudCredentials(secrets), MemoryCloudSettingsStore(CloudSettings(server = FakeCloud.SERVER)))
        val sync = LibrarySync(cloud, lib, null, CoroutineScope(SupervisorJob() + Dispatchers.Default)).also { it.pushDelayMs = 3_600_000 }
        return Triple(lib, cloud, sync)
    }

    @Test
    fun storeTextDropsEveryPlanLink() {
        assertEquals("This plan allows 2 devices.", StoreText.withoutPlanLink("This plan allows 2 devices. See or change the plan at https://cloud.hitop.rws8.tech/orgs/x/plan"))
        assertEquals("Audit export aren't part of this plan.", StoreText.withoutPlanLink("Audit export come with Team and Business plans."))
    }

    @Test
    fun codesFromTheReturnAddressOrPasted() {
        assertEquals("abc", HitopCloud.code("hitop://sign-in?sso=abc"))
        assertEquals("abc", HitopCloud.code("https://x.test/done#sso=abc"))
        assertEquals("abc", HitopCloud.code("  abc "))
        assertNull(HitopCloud.code("hitop://sign-in"))
    }

    @Test
    fun serversMustBeHttpsExceptThisDevice() {
        assertEquals("https://sync.example.com", HitopCloud.normalize("https://sync.example.com/"))
        assertEquals("http://127.0.0.1:7881", HitopCloud.normalize("http://127.0.0.1:7881"))
        assertTrue(runCatching { HitopCloud.normalize("http://sync.example.com") }.isFailure)
        assertEquals(CloudSettings.DEFAULT_SERVER, HitopCloud.normalize(""))
    }

    @Test
    fun usageMetersUseNeutralWords() {
        val u = CloudUsage.of(HitopJson.parseToJsonElement("""{"plan":"free","usage":{"devices":2,"storageBytes":10485760,"seats":1},"limits":{"devices":2,"storageBytes":104857600,"seats":1}}""").jsonObject)
        assertEquals("Free", u.planName)
        assertEquals(listOf("Seats", "Storage", "Devices"), u.meters.map { it.label })
        assertEquals("full", u.meters.first { it.id == "devices" }.level)
        assertEquals("10 MB of 100 MB", u.meters.first { it.id == "storage" }.text)
    }

    @Test
    fun theFirstLinkLeavesTheUntouchedSampleAndAnEmptyMyRequestsHere() = runBlocking {
        val fake = FakeCloud()
        val (lib, cloud, sync) = phone(fake)
        lib.seedFirstVisit()
        val mine = lib.addCollection("mine")
        lib.newRequest(mine.id)
        val (_, paste) = cloud.startBrowserSignIn("github", FakeCloud.SERVER)
        assertFalse(paste)
        cloud.finishBrowserSignIn(fake.approve())
        sync.sync()
        sync.flush()
        val names = fake.items.values.filter { it.kind == "collection" }.map { it.data.jsonObject["name"].toString().trim('"') }
        assertEquals(listOf("mine"), names)
        val sample = lib.state.value.collections.first { it.name == AutoName.SAMPLE_NAME }
        assertTrue(sync.isLocalOnly(sample.id))
        // "Save to HITOP Cloud" sends it after all.
        sync.upload(sample.id)
        assertTrue(fake.items.containsKey("collection/${sample.id}"))
        assertFalse(sync.dirty)
    }

    @Test
    fun theDeviceLimitAddsNothingUntilOneIsRemoved() = runBlocking {
        val fake = FakeCloud().apply { deviceLimit = 1; addDevice("Old phone") }
        val (_, cloud, _) = phone(fake)
        cloud.startBrowserSignIn("github", FakeCloud.SERVER)
        val limit = runCatching { cloud.finishBrowserSignIn(fake.approve()) }.exceptionOrNull() as DeviceLimit
        assertFalse(cloud.signedIn)
        assertFalse(limit.message!!.contains("plan", ignoreCase = true) && limit.message!!.contains("http"))
        val next = cloud.removeDevice(limit.devices.single().id, limit)
        assertTrue(cloud.retry(next))
        assertTrue(cloud.signedIn)
    }
}
