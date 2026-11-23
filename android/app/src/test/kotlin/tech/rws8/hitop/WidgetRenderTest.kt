package tech.rws8.hitop

import androidx.glance.appwidget.testing.unit.runGlanceAppWidgetUnitTest
import androidx.glance.testing.unit.hasContentDescription
import androidx.glance.testing.unit.hasText
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.core.WidgetSnapshot
import tech.rws8.hitop.platform.FailingContent
import tech.rws8.hitop.platform.LastRunContent
import tech.rws8.hitop.platform.PinnedContent

/** A4: the three widgets rendered in Glance's test harness from a snapshot. */
@RunWith(AndroidJUnit4::class)
class WidgetRenderTest {
    private val now = 1_794_912_120_000
    private val snapshot = WidgetSnapshot(
        WidgetSnapshot.LastRun("1001", "shop-api", "Staging", 11, 12, now - 120_000),
        listOf(WidgetSnapshot.CollectionStatus("1001", "shop-api", false, 11, 12), WidgetSnapshot.CollectionStatus("1002", "webhooks", true, 6, 6)),
        listOf(WidgetSnapshot.Pin("1103", "1001", "GET", "List orders", 200), WidgetSnapshot.Pin("1101", "1001", "POST", "Log in", 401)),
    )

    @Test
    fun lastRun() = runGlanceAppWidgetUnitTest {
        provideComposable { LastRunContent(snapshot, now) }
        onNode(hasText("11/12")).assertExists()
        onNode(hasText("Staging · 2 min ago")).assertExists()
    }

    @Test
    fun failing() = runGlanceAppWidgetUnitTest {
        provideComposable { FailingContent(snapshot) }
        onNode(hasText("shop-api")).assertExists()
        onNode(hasText("webhooks")).assertExists()
    }

    @Test
    fun pinnedWithSend() = runGlanceAppWidgetUnitTest {
        provideComposable { PinnedContent(snapshot) }
        onNode(hasText("List orders")).assertExists()
        onNode(hasText("401")).assertExists()
        onNode(hasContentDescription("Send Log in")).assertExists()
    }

    @Test
    fun emptyStates() = runGlanceAppWidgetUnitTest {
        provideComposable { LastRunContent(WidgetSnapshot.EMPTY) }
        onNode(hasText("No run yet. Run a collection in HITOP.")).assertExists()
    }
}
