package tech.rws8.hitop.platform

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import kotlinx.coroutines.launch
import tech.rws8.hitop.MainActivity
import tech.rws8.hitop.R
import tech.rws8.hitop.core.RunProgress

/**
 * A collection run as a foreground service: the run goes on with the app in the background, and
 * its notification shows progress (n of total, passed, failed, environment) with Stop and Open.
 */
class RunService : Service() {
    companion object {
        const val CHANNEL = "runs"
        const val ID = 4201

        /** A run was started here and hasn't ended (main thread only). */
        private var active = false
        const val EXTRA_COLLECTION = "collection"
        const val EXTRA_ITERATIONS = "iterations"
        const val EXTRA_BAIL = "bail"
        const val ACTION_STOP = "tech.rws8.hitop.STOP_RUN"

        fun start(context: Context, collectionId: String, iterations: Int = 1, bail: Boolean = false) {
            val i = Intent(context, RunService::class.java).putExtra(EXTRA_COLLECTION, collectionId).putExtra(EXTRA_ITERATIONS, iterations).putExtra(EXTRA_BAIL, bail)
            context.startForegroundService(i)
        }

        fun channel(context: Context) {
            val nm = context.getSystemService(NotificationManager::class.java)
            nm.createNotificationChannel(NotificationChannel(CHANNEL, "Collection runs", NotificationManager.IMPORTANCE_LOW).apply { description = "Progress of a collection run" })
        }

        fun notification(context: Context, p: RunProgress?): Notification {
            val open = PendingIntent.getActivity(context, 0, Intent(Intent.ACTION_VIEW, Uri.parse("hitop://runs"), context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            val stop = PendingIntent.getService(context, 1, Intent(context, RunService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
            val b = NotificationCompat.Builder(context, CHANNEL).setSmallIcon(R.drawable.ic_notification).setContentIntent(open).setOnlyAlertOnce(true).setCategory(NotificationCompat.CATEGORY_PROGRESS)
            if (p == null) return b.setContentTitle("Starting a run…").build()
            val f = p.finished
            return if (f == null) {
                b.setContentTitle("Running ${p.collectionName} · ${p.done} of ${p.total}")
                    .setContentText("${p.passed} passed · ${p.failed} failed${p.environment?.let { " · $it" } ?: ""}")
                    .setProgress(p.total, p.done, false).setOngoing(true)
                    .addAction(0, "Stop", stop).addAction(0, "Open", open).build()
            } else {
                b.setContentTitle("${p.collectionName}: ${f.passed}/${f.total} passed${if (f.cancelled) " (stopped)" else ""}")
                    .setContentText(f.firstFailure ?: "Every request passed${p.environment?.let { " · $it" } ?: ""}")
                    .setAutoCancel(true).build()
            }
        }
    }

    /** Android 15+: background data sync is over for today. The run stops after its request in flight. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        AppGraph.parts(applicationContext).runs.cancel()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
        stopSelf()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val parts = AppGraph.parts(applicationContext)
        if (intent?.action == ACTION_STOP) {
            parts.runs.cancel()
            return START_NOT_STICKY
        }
        channel(this)
        val started = runCatching { ServiceCompat.startForeground(this, ID, notification(this, parts.runs.current.value?.takeIf { it.finished == null }), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC) }
        if (started.isFailure) {
            // Not allowed now (Android's daily limit for background work): no run, and no crash.
            getSystemService(NotificationManager::class.java).notify(ID, NotificationCompat.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_notification)
                .setContentTitle("The run didn't start").setContentText("Android's limit for background work is used up for now. Open HITOP and try again.").setAutoCancel(true).build())
            stopSelf()
            return START_NOT_STICKY
        }
        // One run at a time: a second start (tile, widget) leaves the one going on alone.
        if (active || parts.runs.running) return START_NOT_STICKY
        val collection = intent?.getStringExtra(EXTRA_COLLECTION) ?: return START_NOT_STICKY.also { stopSelf() }
        val nm = getSystemService(NotificationManager::class.java)
        parts.runs.onProgress = { p -> nm.notify(ID, notification(this, p)) }
        active = true
        AppGraph.scope.launch {
            val record = try {
                runCatching { parts.runs.run(collection, intent.getIntExtra(EXTRA_ITERATIONS, 1), intent.getBooleanExtra(EXTRA_BAIL, false)) }.getOrNull()
            } finally {
                active = false
            }
            Widgets.refresh(applicationContext)
            ServiceCompat.stopForeground(this@RunService, ServiceCompat.STOP_FOREGROUND_DETACH)
            parts.runs.current.value?.let { nm.notify(ID, notification(this@RunService, it)) }
            if (record != null) Shortcuts.update(applicationContext)
            stopSelf()
        }
        return START_NOT_STICKY
    }
}
