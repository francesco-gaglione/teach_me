package com.teachme.feature.lesson

import android.content.Context
import android.content.pm.ApplicationInfo
import android.util.Log
import com.teachme.sdui.model.SduiEvent
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

/** Records what is worth knowing about a lesson document (unknown blocks, wished components...). */
open class SduiEventLogger(
    private val debuggable: () -> Boolean,
) {
    @Inject
    constructor(
        @ApplicationContext context: Context,
    ) : this({ context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0 })

    open fun record(event: SduiEvent) {
        when (event) {
            // The wish list of components is for the team: debug builds only (missing_component.md).
            is SduiEvent.WantedComponent ->
                if (debuggable()) {
                    Log.i(
                        TAG,
                        "wanted component ${event.wanted}: ${event.purpose}",
                    )
                }
            is SduiEvent.UnknownBlockType -> Log.w(TAG, "unknown block type '${event.type}' (id '${event.id}')")
            is SduiEvent.MalformedBlock ->
                Log.w(
                    TAG,
                    "malformed block '${event.id}' of type '${event.type}': ${event.detail}",
                )
            is SduiEvent.DuplicateBlockId -> Log.w(TAG, "duplicate block id '${event.id}' ignored")
            is SduiEvent.NewerCatalog -> Log.w(TAG, "lesson uses catalog v${event.version}, newer than this app")
        }
    }

    private companion object {
        const val TAG = "Sdui"
    }
}
