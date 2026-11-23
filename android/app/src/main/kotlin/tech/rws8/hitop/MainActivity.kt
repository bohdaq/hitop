package tech.rws8.hitop

import android.content.Intent
import android.graphics.Color
import kotlinx.coroutines.launch
import androidx.core.graphics.toColorInt
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.ui.AppViewModel
import tech.rws8.hitop.ui.HitopRoot
import tech.rws8.hitop.ui.theme.HitopTheme

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels {
        object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: Class<T>): T {
                val p = AppGraph.parts(applicationContext)
                return AppViewModel(p.library, p.cloud, p.sync, p.shared, p.signIn, p.runs, p.assistant) as T
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // Dark only: light icons on dark bars, also for 3-button navigation.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark("#15151B".toColorInt()),
        )
        super.onCreate(savedInstanceState)
        setContent { HitopTheme { HitopRoot(vm) } }
        handle(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /** `hitop://sign-in?sso=<code>`: a provider page sent the browser back (HITOP Cloud sign-in). */
    private fun handle(intent: Intent?) {
        val data = intent?.data ?: return
        if (data.scheme != "hitop") return
        when (data.host) {
            "sign-in" -> vm.cloud.handleReturn(data.toString())
            "runs" -> { vm.stack.clear(); vm.tab = tech.rws8.hitop.ui.Tab.RUNS }
            "run" -> data.pathSegments.firstOrNull()?.let { vm.tab = tech.rws8.hitop.ui.Tab.RUNS; vm.push(tech.rws8.hitop.ui.Screen.Run(it)) }
            "request" -> if (data.pathSegments.size == 2) vm.openRequest(data.pathSegments[0], data.pathSegments[1])
        }
        intent.data = null
    }

    override fun onStart() {
        super.onStart()
        // Other devices' changes when the app comes to the front.
        val p = AppGraph.parts(applicationContext)
        AppGraph.scope.launch { p.sync.sync(); p.shared.syncAll() }
    }
}
