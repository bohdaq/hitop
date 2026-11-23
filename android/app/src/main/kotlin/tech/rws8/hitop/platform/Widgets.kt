package tech.rws8.hitop.platform

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.semantics.contentDescription
import androidx.glance.semantics.semantics
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import kotlinx.serialization.json.jsonObject
import tech.rws8.hitop.MainActivity
import tech.rws8.hitop.R
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.WidgetSnapshot
import java.io.File

/** The widgets' data: `widget-snapshot.json` in the app's files, written by [refresh]. */
object Widgets {
    private fun file(context: Context) = File(context.filesDir, "widget-snapshot.json")

    fun snapshot(context: Context): WidgetSnapshot = runCatching { WidgetSnapshot.of(HitopJson.parseToJsonElement(file(context).readText()).jsonObject) }.getOrDefault(WidgetSnapshot.EMPTY)

    /** Writes the snapshot of the library and runs as they are now and redraws every widget. */
    suspend fun refresh(context: Context) {
        val p = AppGraph.parts(context)
        val s = WidgetSnapshot.build(p.library.state.value, p.runs.history.value)
        file(context).writeText(s.toJson().toString())
        runCatching {
            LastRunWidget().updateAll(context)
            FailingWidget().updateAll(context)
            PinnedWidget().updateAll(context)
        }
    }
}

private val ground = ColorProvider(Color(0xFF0F0E15))
private val text = ColorProvider(Color(0xFFECEAF4))
private val muted = ColorProvider(Color(0xFF9C98AE))
private val good = Color(0xFF4ADE80)
private val bad = Color(0xFFF87171)
private val warn = Color(0xFFF59E8B)
private val accent = Color(0xFFA99CFF)

private fun methodColor(m: String) = when (m) {
    "GET" -> Color(0xFF4ADE80); "POST" -> Color(0xFFFBBF24); "PUT" -> Color(0xFF60A5FA); "PATCH" -> Color(0xFFC084FC); "DELETE" -> Color(0xFFF87171); else -> accent
}

private fun openApp(uri: String) = actionStartActivity(Intent(Intent.ACTION_VIEW, Uri.parse(uri)).setClassName("tech.rws8.hitop", MainActivity::class.java.name))

@Composable
private fun Card(content: @Composable () -> Unit) {
    Box(GlanceModifier.fillMaxSize().background(ground).cornerRadius(28.dp).padding(16.dp)) { content() }
}

/** "Last run": the newest run, passed/total, environment, how long ago. */
@Composable
fun LastRunContent(s: WidgetSnapshot, now: Long = System.currentTimeMillis()) {
    Card {
        val r = s.lastRun
        Column(GlanceModifier.fillMaxSize().clickable(openApp(r?.let { "hitop://run/${it.collectionId}" } ?: "hitop://runs"))) {
            Text("Last run", style = TextStyle(color = muted, fontSize = 13.sp))
            if (r == null) {
                Text("No run yet. Run a collection in HITOP.", style = TextStyle(color = text, fontSize = 15.sp))
                return@Column
            }
            Text(r.name, style = TextStyle(color = text, fontSize = 17.sp, fontWeight = FontWeight.Medium), maxLines = 1)
            Text("${r.passed}/${r.total}", style = TextStyle(color = ColorProvider(if (r.passed == r.total) good else warn), fontSize = 34.sp, fontWeight = FontWeight.Bold))
            Text(listOfNotNull(r.environment, LibrarySync.ago(r.at, now)).joinToString(" · "), style = TextStyle(color = muted, fontSize = 13.sp), maxLines = 1)
        }
    }
}

/** "Failing": the collections run lately, failing ones first. */
@Composable
fun FailingContent(s: WidgetSnapshot) {
    Card {
        Column(GlanceModifier.fillMaxSize().clickable(openApp("hitop://runs"))) {
            Text("Failing", style = TextStyle(color = muted, fontSize = 13.sp))
            if (s.collections.isEmpty()) Text("Nothing run yet", style = TextStyle(color = text, fontSize = 15.sp))
            s.collections.sortedBy { it.ok }.take(4).forEach { c ->
                Row(GlanceModifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(GlanceModifier.size(8.dp).cornerRadius(4.dp).background(ColorProvider(if (c.ok) good else bad))) {}
                    Spacer(GlanceModifier.width(8.dp))
                    Text(c.name, style = TextStyle(color = text, fontSize = 15.sp), maxLines = 1)
                }
            }
        }
    }
}

/** "Pinned requests": up to three pins with their last status and a Send button. */
@Composable
fun PinnedContent(s: WidgetSnapshot) {
    Card {
        Column(GlanceModifier.fillMaxSize()) {
            Text("Pinned requests", style = TextStyle(color = muted, fontSize = 13.sp))
            if (s.pins.isEmpty()) Text("Pin a request in HITOP to send it from here.", style = TextStyle(color = text, fontSize = 15.sp))
            s.pins.take(3).forEach { p ->
                Row(GlanceModifier.fillMaxWidth().height(52.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(p.method, style = TextStyle(color = ColorProvider(methodColor(p.method)), fontSize = 12.sp, fontWeight = FontWeight.Medium), modifier = GlanceModifier.width(54.dp))
                    Text(p.name, style = TextStyle(color = text, fontSize = 15.sp), maxLines = 1, modifier = GlanceModifier.defaultWeight().clickable(openApp("hitop://request/${p.collectionId}/${p.requestId}")))
                    p.status?.let { Text("$it", style = TextStyle(color = ColorProvider(if (it < 400) good else bad), fontSize = 13.sp)) }
                    Spacer(GlanceModifier.width(8.dp))
                    Box(
                        GlanceModifier.size(44.dp).cornerRadius(22.dp).background(ColorProvider(Color(0xFF2A2540)))
                            .clickable(actionRunCallback<SendPinned>(actionParametersOf(SendPinned.REQUEST to p.requestId)))
                            .semantics { contentDescription = "Send ${p.name}" },
                        contentAlignment = Alignment.Center,
                    ) {
                        Image(ImageProvider(R.drawable.ic_send), contentDescription = null, colorFilter = ColorFilter.tint(ColorProvider(accent)), modifier = GlanceModifier.size(18.dp))
                    }
                }
            }
        }
    }
}

/** The pinned widget's Send: the request is sent in the app's process, the widget shows its status. */
class SendPinned : ActionCallback {
    companion object {
        val REQUEST = ActionParameters.Key<String>("request")
    }

    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[REQUEST] ?: return
        val p = AppGraph.parts(context)
        val (c, r) = p.library.state.value.find(id) ?: return
        p.library.send(c.id, r)
        Widgets.refresh(context)
    }
}

class LastRunWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val s = Widgets.snapshot(context)
        provideContent { LastRunContent(s) }
    }
}

class FailingWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val s = Widgets.snapshot(context)
        provideContent { FailingContent(s) }
    }
}

class PinnedWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val s = Widgets.snapshot(context)
        provideContent { PinnedContent(s) }
    }
}

class LastRunWidgetReceiver : GlanceAppWidgetReceiver() { override val glanceAppWidget: GlanceAppWidget = LastRunWidget() }
class FailingWidgetReceiver : GlanceAppWidgetReceiver() { override val glanceAppWidget: GlanceAppWidget = FailingWidget() }
class PinnedWidgetReceiver : GlanceAppWidgetReceiver() { override val glanceAppWidget: GlanceAppWidget = PinnedWidget() }
