package com.teachme.feature.review

import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.model.REVIEW_SESSION_SIZE
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewRating
import java.time.Instant

/** How the four grades are laid out (review.md sections 4.4 and 9). */
enum class RatingsLayout { Row, Grid, Column }

private const val ROW_MAX_FONT_SCALE = 1.3f
private const val GRID_MAX_FONT_SCALE = 1.8f

/**
 * One row of four on medium and expanded windows while the font is up to 130 %; a 2 x 2 grid beyond that (up to 180 %),
 * and always on compact windows up to 130 %; a column after that. The order, worst to best, never changes.
 */
fun ratingsLayout(
    width: WindowWidthClass,
    fontScale: Float,
): RatingsLayout =
    when {
        width != WindowWidthClass.Compact && fontScale <= ROW_MAX_FONT_SCALE -> RatingsLayout.Row
        width != WindowWidthClass.Compact && fontScale <= GRID_MAX_FONT_SCALE -> RatingsLayout.Grid
        width == WindowWidthClass.Compact && fontScale <= ROW_MAX_FONT_SCALE -> RatingsLayout.Grid
        else -> RatingsLayout.Column
    }

/** The grades in the fixed order of the screen. */
val RATINGS: List<ReviewRating> = listOf(ReviewRating.Again, ReviewRating.Hard, ReviewRating.Good, ReviewRating.Easy)

/** Above this many characters the back is set in the body size instead of the large card size. */
private const val LONG_BACK_CHARS = 200

fun isLongBack(back: String): Boolean = back.length > LONG_BACK_CHARS

private const val AGAIN_LIST_MAX = 5

/** What the end of the session says (review.md section 7). */
data class FinishSummary(
    /** Votes applied, undone and dropped ones excluded. */
    val reviewed: Int,
    /** Cards graded "Non la ricordavo" (the first [AGAIN_LIST_MAX]); empty when there is a backlog. */
    val again: List<SessionVote>,
    /** How many more "Non la ricordavo" cards there are beyond the list. */
    val againMore: Int,
    /** Cards still due, or null while the backend has not answered. */
    val residual: Int?,
    /** Size of the next session when there is a backlog. */
    val nextSession: Int,
    /** `next_due_at`, only without a backlog. */
    val nextDueAt: Instant?,
    val conflicts: Int,
    /** Title of the topic the session was limited to, or null. */
    val topicTitle: String? = null,
) {
    val hasBacklog: Boolean get() = (residual ?: 0) >= 1
}

fun ReviewUiState.finishSummary(): FinishSummary {
    val backlog = (finish.residual ?: 0) >= 1
    val allAgain = votes.filter { it.rating == ReviewRating.Again }
    return FinishSummary(
        reviewed = votes.size,
        again = if (backlog) emptyList() else allAgain.take(AGAIN_LIST_MAX),
        againMore = if (backlog) 0 else (allAgain.size - AGAIN_LIST_MAX).coerceAtLeast(0),
        residual = finish.residual,
        nextSession = (finish.residual ?: 0).coerceAtMost(REVIEW_SESSION_SIZE),
        nextDueAt = if (backlog) null else finish.nextDueAt,
        conflicts = conflicts,
        topicTitle = topicTitle,
    )
}

/** The pill shows only on cards flagged by the backend (reviews.md section 1.3). */
internal fun showsConsolidatePill(card: ReviewCard): Boolean = card.toConsolidate

/** Announcement of the front: the «to consolidate» sentence is said once, here, not by the pill. */
internal fun frontDescriptionRes(card: ReviewCard): Int =
    if (card.toConsolidate) R.string.review_card_front_consolidate_a11y else R.string.review_card_front_a11y
