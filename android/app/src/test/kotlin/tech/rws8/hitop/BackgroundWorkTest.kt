package tech.rws8.hitop

import android.app.NotificationManager
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.Configuration
import androidx.work.ListenableWorker
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.TestListenableWorkerBuilder
import androidx.work.testing.WorkManagerTestInitHelper
import androidx.work.workDataOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import tech.rws8.hitop.core.MemorySecretStore
import tech.rws8.hitop.core.RunData
import tech.rws8.hitop.core.RunSchedule
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.platform.BackgroundWork
import tech.rws8.hitop.platform.ScheduledRunWorker

/** B5: scheduled runs and background sync. */
@RunWith(AndroidJUnit4::class)
class BackgroundWorkTest {
    private val ctx = ApplicationProvider.getApplicationContext<Context>()

    @Before
    fun seed() {
        AppGraph.useSeed(ctx, MemorySecretStore())
        WorkManagerTestInitHelper.initializeTestWorkManager(ctx, Configuration.Builder().setExecutor(SynchronousExecutor()).build())
    }

    private fun states(name: String) = WorkManager.getInstance(ctx).getWorkInfosForUniqueWork(name).get().map { it.state }

    @Test
    fun aScheduleIsEnqueuedKeptAndTakenAway() {
        val runs = AppGraph.parts(ctx).runs
        runs.setSchedule("1001", RunSchedule("1001", "env-staging", 15))
        BackgroundWork.updateRuns(ctx)
        assertEquals(listOf(WorkInfo.State.ENQUEUED), states(BackgroundWork.runName("1001")))
        BackgroundWork.updateRuns(ctx)
        assertEquals(1, states(BackgroundWork.runName("1001")).size)
        runs.setSchedule("1001", null)
        BackgroundWork.updateRuns(ctx)
        assertEquals(listOf(WorkInfo.State.CANCELLED), states(BackgroundWork.runName("1001")))
        assertEquals(15L, RunSchedule.of(RunSchedule("1", null, 1).toJson()).everyMinutes)
    }

    @Test
    fun aScheduledRunNotifiesOnFailureOnly() = runBlocking {
        val w = TestListenableWorkerBuilder<ScheduledRunWorker>(ctx).setInputData(workDataOf("collection" to "1001", "environment" to "env-staging")).build()
        assertEquals(ListenableWorker.Result.success(), w.doWork())
        val r = AppGraph.parts(ctx).runs.history.value.single()
        assertEquals(11, r.passed)
        assertEquals("Staging", r.environment)
        val n = shadowOf(ctx.getSystemService(NotificationManager::class.java))
        assertEquals(1, n.allNotifications.size)
        assertTrue(n.allNotifications.single().extras.getString("android.text")!!.contains("409"))
    }

    @Test
    fun aDeletedCollectionsScheduleEnds() = runBlocking {
        val runs = AppGraph.parts(ctx).runs
        runs.setSchedule("gone", RunSchedule("gone", null, 60))
        val w = TestListenableWorkerBuilder<ScheduledRunWorker>(ctx).setInputData(workDataOf("collection" to "gone")).build()
        assertEquals(ListenableWorker.Result.success(), w.doWork())
        assertTrue(runs.schedules.value.isEmpty())
    }

    @Test
    fun syncWorkIsThereOnlyWhileSignedInAndOn() {
        val p = AppGraph.parts(ctx)
        BackgroundWork.updateSync(ctx)
        assertTrue(states(BackgroundWork.SYNC).none { it == WorkInfo.State.ENQUEUED })
        val fake = AppGraph.fakeCloud!!
        runBlocking {
            p.cloud.startBrowserSignIn("github", tech.rws8.hitop.cloud.FakeCloud.SERVER)
            p.cloud.finishBrowserSignIn(fake.approve())
        }
        BackgroundWork.updateSync(ctx)
        assertEquals(listOf(WorkInfo.State.ENQUEUED), states(BackgroundWork.SYNC))
        p.settings.update { it.copy(backgroundSync = false) }
        BackgroundWork.updateSync(ctx)
        assertTrue(states(BackgroundWork.SYNC).none { it == WorkInfo.State.ENQUEUED })
    }

    @Test
    fun aDataFileRunsOnceForEachRow() = runBlocking {
        val p = AppGraph.parts(ctx)
        val d = RunData.read("rows.csv", "customerId\n1\n2\n")
        assertEquals(2, d.rows)
        p.runs.setData("1001", d)
        assertEquals(24, p.runs.run("1001")!!.total)
        // Used once.
        assertEquals(12, p.runs.run("1001")!!.total)
    }
}
