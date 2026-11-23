package tech.rws8.hitop

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.sync.VectorChecks

/** A3: the published hitop sync test vectors and the key acceptance policy, through hitop-ffi on the device. */
@RunWith(AndroidJUnit4::class)
class VectorTest {
    @Test
    fun publishedVectorsPassOnTheDevice() {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val n = VectorChecks.run { name -> assets.open(name).bufferedReader().readText() }
        assertTrue("$n cases", n >= 30)
    }
}
