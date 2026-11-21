package com.teachme.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.teachme.app.reminders.ReviewIntents
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.FlashInbox
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var flashInbox: FlashInbox

    /** Set by the tap on the review notification (ADR 0012 point 5), cleared once the navigation host has acted on it. */
    private var openReview by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // The first start reads the intent. After a rotation or a restore the saved request is used instead: still pending
        // if the navigation host had not served it yet, false if it had (so it is consumed once).
        openReview =
            ReviewIntents.initialRequest(
                savedInstanceState?.getBoolean(ReviewIntents.STATE_OPEN_REVIEW),
                ReviewIntents.isOpenReview(intent),
            )
        setContent {
            TeachTheme {
                TeachMeApp(flashInbox, openReview = openReview, onOpenReviewHandled = { openReview = false })
            }
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putBoolean(ReviewIntents.STATE_OPEN_REVIEW, openReview)
    }

    /** The app was already open (single top): the tap arrives here instead of starting another copy. */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (ReviewIntents.isOpenReview(intent)) openReview = true
    }
}
