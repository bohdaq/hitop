package tech.rws8.hitop

import android.app.Application
import tech.rws8.hitop.platform.CrashReport
import tech.rws8.hitop.platform.Strings

class HitopApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Strings.install(this)
        CrashReport.install(this)
    }
}
