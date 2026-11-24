package tech.rws8.hitop.platform

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import tech.rws8.hitop.MainActivity
import tech.rws8.hitop.R
import tech.rws8.hitop.core.RunRecord
import tech.rws8.hitop.core.RunSchedule
import java.util.concurrent.TimeUnit

/** Work while HITOP is closed: the library's sync and scheduled runs (WorkManager; approximate times). */
object BackgroundWork {
    const val SYNC = "library-sync"
    const val PUSH = "library-push"
    fun runName(collectionId: String) = "run-$collectionId"
    private const val RUN_TAG = "scheduled-run"

    private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED)

    /** Sync every 6 hours while signed in and both switches are on, else not at all. */
    fun updateSync(context: Context) {
        val p = AppGraph.parts(context)
        val wm = WorkManager.getInstance(context)
        val s = p.settings.value
        if (p.cloud.signedIn && s.librarySync && s.backgroundSync) {
            wm.enqueueUniquePeriodicWork(SYNC, ExistingPeriodicWorkPolicy.UPDATE, PeriodicWorkRequestBuilder<SyncWorker>(6, TimeUnit.HOURS).setConstraints(online.setRequiresBatteryNotLow(true).build()).build())
        } else {
            wm.cancelUniqueWork(SYNC)
            wm.cancelUniqueWork(PUSH)
        }
    }

    /** Changes that didn't reach HITOP Cloud go when there is a network. */
    fun pushLater(context: Context) {
        val p = AppGraph.parts(context)
        val s = p.settings.value
        if (!p.cloud.signedIn || !s.librarySync || !s.backgroundSync || !p.sync.dirty) return
        WorkManager.getInstance(context).enqueueUniqueWork(PUSH, ExistingWorkPolicy.KEEP, OneTimeWorkRequestBuilder<SyncWorker>().setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()).build())
    }

    fun updateRuns(context: Context) {
        val wm = WorkManager.getInstance(context)
        val list = AppGraph.parts(context).runs.schedules.value
        val names = list.map { runName(it.collectionId) }.toSet()
        // Every scheduled run is tagged with its own name, so those no longer wanted are found.
        val infos = wm.getWorkInfosByTag(RUN_TAG)
        infos.addListener({
            runCatching { infos.get() }.getOrNull()?.filter { !it.state.isFinished }?.flatMap { it.tags }?.filter { it.startsWith("run-") && it !in names }?.forEach { wm.cancelUniqueWork(it) }
        }, Runnable::run)
        list.forEach { s -> schedule(wm, s) }
    }

    private fun schedule(wm: WorkManager, s: RunSchedule) {
        val r = PeriodicWorkRequestBuilder<ScheduledRunWorker>(s.everyMinutes, TimeUnit.MINUTES)
            .setInitialDelay(s.everyMinutes, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInputData(workDataOf("collection" to s.collectionId, "environment" to s.environmentId))
            .addTag(RUN_TAG).addTag(runName(s.collectionId)).build()
        wm.enqueueUniquePeriodicWork(runName(s.collectionId), ExistingPeriodicWorkPolicy.UPDATE, r)
    }

    fun notifyFailure(context: Context, r: RunRecord) {
        RunService.channel(context)
        val open = PendingIntent.getActivity(context, r.collectionId.hashCode(), Intent(Intent.ACTION_VIEW, Uri.parse("hitop://runs"), context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(context, RunService.CHANNEL).setSmallIcon(R.drawable.ic_notification).setContentIntent(open).setAutoCancel(true)
            .setContentTitle("${r.collectionName}: ${r.passed}/${r.total} passed").setContentText(r.firstFailure ?: "The scheduled run failed").build()
        context.getSystemService(NotificationManager::class.java).notify(r.collectionId.hashCode(), n)
    }
}

class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.Main) {
        val p = AppGraph.parts(applicationContext)
        if (!p.cloud.signedIn || !p.settings.value.librarySync || !p.settings.value.backgroundSync) return@withContext Result.success()
        try {
            p.sync.sync()
            p.sync.flush()
            p.shared.syncAll()
            p.shared.flush()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            return@withContext Result.retry()
        }
        if (p.sync.status.value.error != null && p.sync.dirty && runAttemptCount < 5) Result.retry() else Result.success()
    }
}

class ScheduledRunWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val p = AppGraph.parts(applicationContext)
        val id = inputData.getString("collection") ?: return Result.failure()
        if (p.library.state.value.collection(id) == null) {
            p.runs.setSchedule(id, null)
            return Result.success()
        }
        val record = try {
            p.runs.run(id, environmentId = inputData.getString("environment"))
        } catch (e: CancellationException) {
            // Android took the time back: the run stops after its request in flight.
            p.runs.cancel()
            throw e
        } catch (_: Exception) {
            null
        }
        // Another run was going on: later.
        if (record == null) return if (runAttemptCount < 3) Result.retry() else Result.success()
        Widgets.refresh(applicationContext)
        if (!record.ok && !record.cancelled) BackgroundWork.notifyFailure(applicationContext, record)
        return Result.success()
    }
}
