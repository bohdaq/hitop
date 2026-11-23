package tech.rws8.hitop

import android.graphics.Color
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
            override fun <T : ViewModel> create(modelClass: Class<T>): T = AppViewModel(AppGraph.library(applicationContext)) as T
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
    }
}
