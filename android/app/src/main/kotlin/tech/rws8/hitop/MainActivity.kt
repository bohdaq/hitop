package tech.rws8.hitop

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopChip
import tech.rws8.hitop.ui.theme.HitopTheme
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.LogoMark
import tech.rws8.hitop.ui.theme.MethodTag

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent { HitopTheme { Home() } }
    }
}

@Composable
private fun Home() {
    var engine by remember { mutableStateOf("Starting the engine…") }
    LaunchedEffect(Unit) {
        engine = withContext(Dispatchers.Default) {
            runCatching {
                val out = Ffi.call("evalScript", buildJsonObject { put("script", "pm.environment.set('answer', String(6 * 7))") })
                "Engine ready · script answer " + out.jsonObject["variables"]!!.jsonObject["answer"]!!.jsonPrimitive.content
            }.getOrElse { "Engine failed: ${it.message}" }
        }
    }
    Column(
        Modifier.fillMaxSize().background(Hitop.ground).safeDrawingPadding().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            LogoMark()
            Column {
                Text("Collections", style = HitopType.screenTitle, color = Hitop.text)
                Text("On this phone", style = HitopType.caption, color = Hitop.textMuted)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            HitopChip("All", selected = true)
            HitopChip("Pinned", selected = false)
            HitopChip("Recent", selected = false)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            listOf("GET", "POST", "PUT", "PATCH", "DELETE").forEach { MethodTag(it) }
        }
        Text(engine, Modifier.testTag("engine"), style = HitopType.caption, color = Hitop.textMuted)
    }
}
