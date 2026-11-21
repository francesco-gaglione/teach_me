package com.teachme.feature.review

import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewRating
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReviewLogicTest {
    @Test
    fun `wide windows keep one row of four up to 130 percent, then 2 by 2, then a column`() {
        assertEquals(RatingsLayout.Row, ratingsLayout(WindowWidthClass.Expanded, 1.0f))
        assertEquals(RatingsLayout.Row, ratingsLayout(WindowWidthClass.Medium, 1.3f))
        assertEquals(RatingsLayout.Grid, ratingsLayout(WindowWidthClass.Medium, 1.31f))
        assertEquals(RatingsLayout.Grid, ratingsLayout(WindowWidthClass.Expanded, 1.8f))
        assertEquals(RatingsLayout.Column, ratingsLayout(WindowWidthClass.Expanded, 2.0f))
    }

    @Test
    fun `compact is 2 by 2 up to 130 percent, then a column`() {
        assertEquals(RatingsLayout.Grid, ratingsLayout(WindowWidthClass.Compact, 1.0f))
        assertEquals(RatingsLayout.Grid, ratingsLayout(WindowWidthClass.Compact, 1.3f))
        assertEquals(RatingsLayout.Column, ratingsLayout(WindowWidthClass.Compact, 1.31f))
        assertEquals(RatingsLayout.Column, ratingsLayout(WindowWidthClass.Compact, 2.0f))
    }

    @Test
    fun `the grades keep a fixed order, worst to best`() {
        assertEquals(
            listOf(ReviewRating.Again, ReviewRating.Hard, ReviewRating.Good, ReviewRating.Easy),
            RATINGS,
        )
    }

    @Test
    fun `a back over 200 characters is set in the body size`() {
        assertFalse(isLongBack("x".repeat(200)))
        assertTrue(isLongBack("x".repeat(201)))
    }

    private fun vote(
        id: String,
        rating: ReviewRating,
    ) = SessionVote(card(id), rating, 600, QueuedReview(id, rating.wire, 1L, 0))

    @Test
    fun `the list of cards to see soon holds five and says how many more`() {
        val state =
            ReviewUiState(
                phase = ReviewPhase.Finished,
                votes =
                    (1..7).map { vote("c$it", ReviewRating.Again) } + vote("g", ReviewRating.Good) +
                        vote("h", ReviewRating.Hard),
                finish = FinishData(residual = 0),
            )
        val summary = state.finishSummary()
        assertEquals(9, summary.reviewed)
        assertEquals(5, summary.again.size)
        assertEquals(2, summary.againMore)
        // "Con fatica" and "Bene" are not in the list.
        assertTrue(summary.again.all { it.rating == ReviewRating.Again })
    }

    @Test
    fun `the next session is at most 20 and singular for the last card`() {
        val base = ReviewUiState(phase = ReviewPhase.Finished, votes = listOf(vote("a", ReviewRating.Good)))
        assertEquals(20, base.copy(finish = FinishData(residual = 87)).finishSummary().nextSession)
        assertEquals(7, base.copy(finish = FinishData(residual = 7)).finishSummary().nextSession)
        assertEquals(1, base.copy(finish = FinishData(residual = 1)).finishSummary().nextSession)
        assertFalse(base.copy(finish = FinishData(residual = 0)).finishSummary().hasBacklog)
        // Totals not arrived yet: no backlog is claimed.
        assertFalse(base.finishSummary().hasBacklog)
    }

    @Test
    fun `a card to consolidate shows the pill and the longer announcement, a normal one neither`() {
        val flagged = card("1").copy(toConsolidate = true)
        assertTrue(showsConsolidatePill(flagged))
        assertEquals(R.string.review_card_front_consolidate_a11y, frontDescriptionRes(flagged))
        assertFalse(showsConsolidatePill(card("2")))
        assertEquals(R.string.review_card_front_a11y, frontDescriptionRes(card("2")))
    }
}
