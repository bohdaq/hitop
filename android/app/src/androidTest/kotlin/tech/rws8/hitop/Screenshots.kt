package tech.rws8.hitop

import android.graphics.Bitmap
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage

/** Screen captures into the test storage (CI artifact "screenshots"), named per API level. */
object Screenshots {
    fun take(name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.waitForIdleSync()
        val bitmap = instrumentation.uiAutomation.takeScreenshot() ?: return
        TestStorage().openOutputFile("screenshots/api${android.os.Build.VERSION.SDK_INT}/$name.png").use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}
