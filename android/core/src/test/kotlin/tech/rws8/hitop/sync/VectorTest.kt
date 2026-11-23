package tech.rws8.hitop.sync

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class VectorTest {
    @Test
    fun publishedVectorsPassOnTheJvm() {
        val dir = File(System.getenv("HITOP_REPO") ?: "../..", "cli/crates/hitop-sync-crypto/vectors")
        val n = VectorChecks.run { File(dir, it).readText() }
        assertTrue("$n cases", n >= 30)
    }
}
