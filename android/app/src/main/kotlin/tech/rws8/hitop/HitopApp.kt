package tech.rws8.hitop

import android.app.Application
import tech.rws8.hitop.platform.CrashReport

class HitopApp : Application() {
    override fun onCreate() {
        super.onCreate()
        CrashReport.install(this)
    }
}
