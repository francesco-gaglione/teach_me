package com.teachme.feature.review

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.IntervalsPreview
import com.teachme.core.model.LoadError
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewRating
import com.teachme.core.model.ReviewState
import com.teachme.core.ui.DialogUi
import java.time.Instant

private val NOW: Instant = Instant.parse("2026-11-10T09:00:00Z")

private fun card(
    id: String,
    front: String,
    back: String,
    kind: ReviewCardKind = ReviewCardKind.Record,
    toConsolidate: Boolean = false,
) = ReviewCard(
    id = id,
    topicId = "t",
    topicTitle = "Rust",
    lessonId = "l",
    lessonNumber = 2,
    lessonTitle = "Result e operatore ?",
    kind = kind,
    front = front,
    back = back,
    status = ReviewCardStatus.Active,
    toConsolidate = toConsolidate,
    state = ReviewState.Review,
    due = NOW,
    lastReviewedAt = null,
    reviewCount = 3,
    intervals = IntervalsPreview(600, 172_800, 432_000, 950_400),
)

private val SAMPLE_CARDS =
    listOf(
        card(
            "1",
            "Quando compila l'operatore `?` dentro una funzione?",
            "Solo se il tipo di ritorno può contenere l'errore, per esempio `Result<T, E>`.",
        ),
        card(
            "2",
            "Che cos'è il *borrowing*?",
            "Prendere in prestito un valore **senza** prenderne la proprietà.",
            ReviewCardKind.Term,
        ),
        card("3", "Cosa fa `clone()`?", "Duplica il valore."),
    )

private fun vote(
    card: ReviewCard,
    rating: ReviewRating,
) = SessionVote(
    card,
    rating,
    card.intervals?.forRating(rating),
    QueuedReview(card.id, rating.wire, 1L, card.reviewCount),
)

private val CONSOLIDATE_CARDS =
    listOf(
        card(
            "4",
            "Quando compila l'operatore `?`?",
            "Solo dentro una funzione che restituisce `Result`.",
            toConsolidate = true,
        ),
        card(
            "5",
            "Che cos'è il *borrowing*?",
            "Prendere in prestito un valore **senza** la proprietà.",
            ReviewCardKind.Term,
            toConsolidate = true,
        ),
    )

private val FRONT = ReviewUiState(phase = ReviewPhase.Card, cards = SAMPLE_CARDS, index = 0)
private val BACK = FRONT.copy(revealed = true)
private val AFTER_VOTE =
    ReviewUiState(
        phase = ReviewPhase.Card,
        cards = SAMPLE_CARDS,
        index = 1,
        votes = listOf(vote(SAMPLE_CARDS[0], ReviewRating.Good)),
        notice = ReviewNotice.Voted(ReviewRating.Good, 432_000),
        undoableUntilMs = Long.MAX_VALUE,
        pendingCount = 1,
    )
private val FINISHED =
    ReviewUiState(
        phase = ReviewPhase.Finished,
        cards = SAMPLE_CARDS,
        index = 3,
        votes =
            listOf(
                vote(SAMPLE_CARDS[0], ReviewRating.Again),
                vote(SAMPLE_CARDS[1], ReviewRating.Good),
                vote(SAMPLE_CARDS[2], ReviewRating.Hard),
            ),
        finish = FinishData(residual = 0, nextDueAt = Instant.parse("2026-11-11T09:00:00Z")),
    )

@Composable
private fun Host(
    state: ReviewUiState,
    dark: Boolean,
) = TeachTheme(darkTheme = dark) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        ReviewScreen(state, ReviewActions(), canUndo = { true }, now = NOW)
    }
}

private const val TABLET = "spec:width=1280dp,height=800dp,dpi=240"
private const val PHONE = "spec:width=360dp,height=640dp,dpi=420"

@Preview(name = "Front expanded", device = TABLET, showBackground = true)
@Composable
private fun FrontExpandedPreview() = Host(FRONT, false)

@Preview(name = "Back expanded (right answer: the four grades)", device = TABLET, showBackground = true)
@Composable
private fun BackExpandedPreview() = Host(BACK, false)

@Preview(name = "Back expanded dark", device = TABLET, showBackground = true)
@Composable
private fun BackExpandedDarkPreview() = Host(BACK, true)

@Preview(name = "To consolidate: front", device = TABLET, showBackground = true)
@Composable
private fun ConsolidateFrontPreview() = Host(ReviewUiState(phase = ReviewPhase.Card, cards = CONSOLIDATE_CARDS), false)

@Preview(name = "To consolidate: back", device = TABLET, showBackground = true)
@Composable
private fun ConsolidateBackPreview() =
    Host(ReviewUiState(phase = ReviewPhase.Card, cards = CONSOLIDATE_CARDS, revealed = true), false)

@Preview(name = "To consolidate: term front dark", device = TABLET, showBackground = true)
@Composable
private fun ConsolidateTermDarkPreview() =
    Host(ReviewUiState(phase = ReviewPhase.Card, cards = CONSOLIDATE_CARDS, index = 1), true)

@Preview(name = "After a vote: UndoRow", device = TABLET, showBackground = true)
@Composable
private fun AfterVotePreview() = Host(AFTER_VOTE, false)

@Preview(name = "Offline queue", device = TABLET, showBackground = true)
@Composable
private fun OfflineQueuePreview() = Host(FRONT.copy(pendingCount = 2), false)

@Preview(name = "Suspend dialog", device = TABLET, showBackground = true)
@Composable
private fun SuspendPreview() = Host(FRONT.copy(suspendDialog = DialogUi()), false)

@Preview(name = "Finished", device = TABLET, showBackground = true)
@Composable
private fun FinishedPreview() = Host(FINISHED, false)

@Preview(name = "Finished dark", device = TABLET, showBackground = true)
@Composable
private fun FinishedDarkPreview() = Host(FINISHED, true)

@Preview(name = "Finished with a backlog", device = TABLET, showBackground = true)
@Composable
private fun FinishedBacklogPreview() = Host(FINISHED.copy(finish = FinishData(residual = 34), conflicts = 1), false)

@Preview(name = "Nothing due", device = TABLET, showBackground = true)
@Composable
private fun EmptyPreview() =
    Host(ReviewUiState(phase = ReviewPhase.Empty, emptyNextDueAt = Instant.parse("2026-11-11T09:00:00Z")), false)

@Preview(name = "Medium front", device = "spec:width=720dp,height=1000dp,dpi=240", showBackground = true)
@Composable
private fun MediumFrontPreview() = Host(FRONT, false)

@Preview(name = "Medium back", device = "spec:width=720dp,height=1000dp,dpi=240", showBackground = true)
@Composable
private fun MediumBackPreview() = Host(BACK, false)

@Preview(name = "Compact front", device = PHONE, showBackground = true)
@Composable
private fun CompactFrontPreview() = Host(FRONT, false)

@Preview(name = "Compact back", device = PHONE, showBackground = true)
@Composable
private fun CompactBackPreview() = Host(BACK, false)

@Preview(name = "Compact back dark", device = PHONE, showBackground = true)
@Composable
private fun CompactBackDarkPreview() = Host(BACK, true)

@Preview(name = "Compact back font 2.0", device = PHONE, showBackground = true, fontScale = 2f)
@Composable
private fun CompactBackLargePreview() = Host(BACK, false)

@Preview(name = "Compact loading", device = PHONE, showBackground = true)
@Composable
private fun LoadingPreview() = Host(ReviewUiState(phase = ReviewPhase.Loading, slowLoading = true), false)

@Preview(name = "Compact error", device = PHONE, showBackground = true)
@Composable
private fun ErrorPreview() = Host(ReviewUiState(phase = ReviewPhase.Failed, loadError = LoadError.Network), true)
