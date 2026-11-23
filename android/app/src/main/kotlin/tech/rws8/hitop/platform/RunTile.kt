package tech.rws8.hitop.platform

import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/** Quick Settings: run the collection that ran last ("Run shop-api"). */
class RunTile : TileService() {
    override fun onStartListening() {
        val last = Widgets.snapshot(applicationContext).lastRun
        qsTile?.apply {
            label = last?.let { "Run ${it.name}" } ?: "Run a collection"
            subtitle = last?.let { "${it.passed}/${it.total} last time" }
            state = if (last == null) Tile.STATE_UNAVAILABLE else Tile.STATE_INACTIVE
            updateTile()
        }
    }

    override fun onClick() {
        val last = Widgets.snapshot(applicationContext).lastRun ?: return
        RunService.start(applicationContext, last.collectionId)
    }
}
