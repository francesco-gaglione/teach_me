package com.teachme.feature.settings

import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.compose.runtime.remember
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocusable
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.LoadError
import com.teachme.core.model.SuspendedReviews
import com.teachme.core.ui.SecretField
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.time.Instant
import java.time.ZoneId

private val NOW = Instant.parse("2026-11-16T10:00:00Z")

private fun rows(vararg phases: RowPhase) =
    phases.mapIndexed { i, phase -> SuspendedRow(card("c$i", front = "Domanda $i"), phase) }

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class SuspendedScreenUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val reactivated = mutableListOf<String>()
    private var retried = 0
    private var loadedMore = 0

    private fun show(
        state: SuspendedUiState,
        dark: Boolean = false,
    ) {
        rule.setContent {
            TeachTheme(darkTheme = dark) {
                SuspendedScreen(
                    state = state,
                    toast = remember { TeachToastState() },
                    onBack = {},
                    onRetry = { retried++ },
                    onLoadMore = { loadedMore++ },
                    onReactivate = { reactivated += it },
                    now = NOW,
                    zone = ZoneId.of("UTC"),
                )
            }
        }
    }

    @Test
    fun `rows show the front, the origin, when, and one Riattiva each`() {
        show(SuspendedUiState(rows(RowPhase.Idle, RowPhase.Idle), count = 2))

        rule.onNodeWithText("2 schede sospese").assertIsDisplayed()
        rule.onNodeWithText("Domanda 0").assertIsDisplayed()
        rule.onAllNodesWithText("Rust · Lezione 2: Result").assertCountEquals(2)
        rule.onAllNodesWithText("Sospesa ieri").assertCountEquals(2)
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 0").assertIsDisplayed()
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 1").assertIsDisplayed()
    }

    @Test
    fun `the count is singular for one card`() {
        show(SuspendedUiState(rows(RowPhase.Idle), count = 1))
        rule.onNodeWithText("1 scheda sospesa").assertIsDisplayed()
    }

    @Test
    fun `the row is one readable group and the button is its own target`() {
        show(SuspendedUiState(rows(RowPhase.Idle), count = 1))

        val group =
            rule.onNode(hasContentDescription("Domanda 0. Rust, lezione 2, Result. Sospesa ieri."))
        group.assertIsDisplayed()
        val actions =
            group
                .fetchSemanticsNode()
                .config
                .getOrNull(SemanticsActions.CustomActions)
                .orEmpty()
        assertEquals(listOf("Riattiva"), actions.map { it.label })
        // The group itself is not a button: the only click target is the real one.
        assertEquals(null, group.fetchSemanticsNode().config.getOrNull(SemanticsProperties.Role))
        assertEquals(null, group.fetchSemanticsNode().config.getOrNull(SemanticsActions.OnClick))
    }

    @Test
    fun `tapping Riattiva and the custom action both ask for that card`() {
        show(SuspendedUiState(rows(RowPhase.Idle, RowPhase.Idle), count = 2))

        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 1").performClick()
        assertEquals(listOf("c1"), reactivated)

        val group = rule.onNode(hasContentDescription("Domanda 0. Rust, lezione 2, Result. Sospesa ieri."))
        val action = group.fetchSemanticsNode().config[SemanticsActions.CustomActions].first()
        rule.runOnUiThread { action.action() }
        assertEquals(listOf("c1", "c0"), reactivated)
    }

    @Test
    fun `a row being reactivated says so in words and does not send again`() {
        show(SuspendedUiState(rows(RowPhase.Busy, RowPhase.Idle), count = 2))

        rule.onNodeWithContentDescription("Riattivo la scheda: Domanda 0").performClick()
        assertEquals(emptyList<String>(), reactivated)
        // The other row stays usable.
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 1").performClick()
        assertEquals(listOf("c1"), reactivated)
    }

    @Test
    fun `a failed row explains and offers Riprova`() {
        show(SuspendedUiState(rows(RowPhase.Failed), count = 1))

        rule.onNodeWithText("Non riesco a raggiungere il backend. La scheda è ancora sospesa.").assertIsDisplayed()
        rule.onNodeWithContentDescription("Riprova a riattivare la scheda: Domanda 0").performClick()
        assertEquals(listOf("c0"), reactivated)
    }

    @Test
    fun `no cards shows the empty state without buttons`() {
        show(SuspendedUiState(rows = emptyList()))

        rule.onNodeWithText("Nessuna scheda sospesa").assertIsDisplayed()
        rule
            .onNodeWithText(
                "Le schede che sospendi dal ripasso compaiono qui, e da qui le riattivi.",
            ).assertIsDisplayed()
        rule.onAllNodesWithText("Riprova").assertCountEquals(0)
    }

    @Test
    fun `the first load is a described skeleton, with the slow text after a while`() {
        show(SuspendedUiState(slowLoading = true))

        rule.onNode(hasContentDescription("Carico le schede sospese")).assertIsDisplayed()
        rule.onNodeWithText("Sto ancora caricando…").assertIsDisplayed()
    }

    @Test
    fun `a load error has Riprova`() {
        show(SuspendedUiState(loadError = LoadError.Network))

        rule.onNodeWithText("Non riesco a raggiungere il backend").assertIsDisplayed()
        rule
            .onNodeWithText(
                "Controlla che il dispositivo sia collegato alla rete e che il backend sia acceso.",
            ).assertIsDisplayed()
        rule.onNodeWithText("Riprova").performClick()
        assertEquals(1, retried)
    }

    @Test
    fun `a server error has its own words`() {
        show(SuspendedUiState(loadError = LoadError.Server))
        rule.onNodeWithText("Il backend ha avuto un problema").assertIsDisplayed()
    }

    @Test
    fun `the tail shows loading and a failed page can be retried`() {
        show(SuspendedUiState(rows(RowPhase.Idle), count = 2, nextCursor = "c", pageLoading = true))
        rule.onNodeWithText("Carico altre schede").assertIsDisplayed()
    }

    @Test
    fun `a failed page says so and retries`() {
        show(SuspendedUiState(rows(RowPhase.Idle), count = 2, nextCursor = "c", pageFailed = true))
        rule.onNodeWithText("Non riesco a caricarne altre").assertIsDisplayed()
        val before = loadedMore
        rule.onNode(hasText("Riprova")).performClick()
        assertEquals(before + 1, loadedMore)
    }

    @Test
    fun `it also draws in the dark theme`() {
        show(SuspendedUiState(rows(RowPhase.Idle), count = 1), dark = true)
        rule.onNodeWithText("Domanda 0").assertIsDisplayed()
    }
}

/** The "Ripasso" section of Settings: only with something to reactivate. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h1200dp-xxhdpi")
class SettingsReviewSectionUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private var opened = 0

    private fun show(count: Int?) {
        rule.setContent {
            TeachTheme {
                SettingsScreen(
                    state = SettingsUiState(user = USER, host = "teach.tailnet.ts.net", suspendedCount = count),
                    keyField = remember { SecretField() },
                    toast = remember { TeachToastState() },
                    actions = SettingsActions(onOpenSuspended = { opened++ }),
                )
            }
        }
    }

    @Test
    fun `with suspended cards the section opens the list`() {
        show(3)

        rule.onNodeWithText("Ripasso").assertIsDisplayed()
        rule.onNodeWithText("3").assertIsDisplayed()
        rule.onNodeWithContentDescription("Schede sospese, 3. Apre l'elenco.").performClick()
        assertEquals(1, opened)
    }

    @Test
    fun `while the count is unknown or zero there is no Ripasso section`() {
        show(null)
        rule.onAllNodesWithText("Ripasso").assertCountEquals(0)
        rule.onAllNodesWithText("Schede sospese").assertCountEquals(0)
    }

    @Test
    fun `zero cards hide the section too`() {
        show(0)
        rule.onAllNodesWithText("Ripasso").assertCountEquals(0)
    }

    @Test
    fun `the section sits between Password and Notifiche`() {
        show(2)
        val top = { text: String ->
            rule
                .onNodeWithText(text)
                .fetchSemanticsNode()
                .positionInRoot.y
        }
        assert(top("Password") < top("Ripasso"))
        assert(top("Ripasso") < top("Notifiche"))
    }
}

/** The route: toast, Ctrl+Z and the row leaving, with a real ViewModel over a fake repository. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class SuspendedRouteUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val reviews = FakeReviews()

    private fun show(reducedMotion: Boolean = false) {
        reviews.pages +=
            ApiResult.Success(
                SuspendedReviews(listOf("c0", "c1", "c2").map { card(it, front = "Domanda ${it.last()}") }, null, 3),
            )
        Settings.Global.putFloat(
            rule.activity.contentResolver,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            if (reducedMotion) 0f else 1f,
        )
        val viewModel = SuspendedViewModel(reviews, AppClock { NOW.toEpochMilli() })
        rule.setContent {
            TeachTheme { SuspendedRoute(onBack = {}, viewModel = viewModel, now = NOW, zone = ZoneId.of("UTC")) }
        }
        rule.waitUntil(WAIT_MS) { rule.onAllNodesWithText("Domanda 0").fetchSemanticsNodes().isNotEmpty() }
    }

    private fun reactivateFirst() {
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 0").performClick()
        rule.waitUntil(WAIT_MS) { rule.onAllNodesWithText("Annulla").fetchSemanticsNodes().isNotEmpty() }
    }

    private fun rowGone(front: String) = rule.onAllNodesWithText(front).fetchSemanticsNodes().isEmpty()

    @Test
    fun `reactivating shows a toast with Annulla for 6 seconds and then hides it`() {
        show()
        reactivateFirst()

        rule.onNodeWithText("Scheda riattivata", substring = true).assertIsDisplayed()
        rule.mainClock.autoAdvance = false
        rule.mainClock.advanceTimeBy(5_500)
        rule.onNodeWithText("Annulla").assertIsDisplayed()
        rule.mainClock.advanceTimeBy(1_000)
        rule.onAllNodesWithText("Annulla").assertCountEquals(0)
    }

    @Test
    fun `Annulla brings the row back on top`() {
        show()
        reactivateFirst()
        rule.onNodeWithText("Annulla").performClick()
        rule.waitUntil(WAIT_MS) { !rowGone("Fronte c0") }
        rule.onAllNodesWithText("Annulla").assertCountEquals(0)
        assertEquals(listOf("c0"), reviews.suspendedAgain)
    }

    @Test
    fun `Ctrl+Z does what Annulla does while the toast is visible`() {
        show()
        reactivateFirst()
        rule.waitForIdle()

        rule.onRoot().performKeyInput {
            keyDown(Key.CtrlLeft)
            keyDown(Key.Z)
            keyUp(Key.Z)
            keyUp(Key.CtrlLeft)
        }
        rule.waitUntil(WAIT_MS) { reviews.suspendedAgain.isNotEmpty() }
        assertEquals(listOf("c0"), reviews.suspendedAgain)
    }

    @Test
    fun `after reactivating, focus moves to the next row`() {
        show()
        reactivateFirst()
        rule.waitForIdle()
        rule.onNode(hasContentDescription("Domanda 1. Rust, lezione 2, Result. Sospesa ieri.")).assertIsFocused()
    }

    @Test
    fun `after reactivating the last row, focus moves to the title`() {
        show()
        reactivateFirst()
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 1").performClick()
        rule.waitUntil(WAIT_MS) { rowGone("Fronte c1") && rowGone("Domanda 1") }
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 2").performClick()
        rule.waitUntil(WAIT_MS) { rowGone("Domanda 2") }
        rule.waitForIdle()
        rule.onNode(hasText("Schede sospese") and isFocusable()).assertIsFocused()
    }

    private fun topOf(front: String) =
        rule
            .onNodeWithText(front)
            .fetchSemanticsNode()
            .positionInRoot.y

    private fun tapReactivateAndWaitForAnswer() {
        rule.onNodeWithContentDescription("Riattiva la scheda: Domanda 0").performClick()
        // The request answers on the main looper: give it frames until the row starts leaving.
        var frames = 0
        while (reviews.unsuspended.isEmpty() && frames++ < MAX_FRAMES) rule.mainClock.advanceTimeByFrame()
    }

    @Test
    fun `the rows below slide up over 200 ms instead of jumping`() {
        show()
        val before = topOf("Domanda 1")
        rule.mainClock.autoAdvance = false
        tapReactivateAndWaitForAnswer()
        rule.mainClock.advanceTimeBy(80)
        val mid = topOf("Domanda 1")
        rule.mainClock.advanceTimeBy(400)
        val after = topOf("Domanda 1")

        assertTrue("moved from $before", after < before)
        assertTrue("mid $mid between $after and $before", mid > after && mid <= before)
    }

    @Test
    fun `with reduce animations the rows below jump at once`() {
        show(reducedMotion = true)
        val before = topOf("Domanda 1")
        rule.mainClock.autoAdvance = false
        tapReactivateAndWaitForAnswer()
        rule.mainClock.advanceTimeBy(48)
        val after = topOf("Domanda 1")
        assertTrue(after < before)
        rule.mainClock.advanceTimeBy(400)
        assertEquals(after, topOf("Domanda 1"), 0.5f)
        assertEquals(true, rowGone("Domanda 0"))
    }

    private companion object {
        const val WAIT_MS = 5_000L
        const val MAX_FRAMES = 60
    }
}
