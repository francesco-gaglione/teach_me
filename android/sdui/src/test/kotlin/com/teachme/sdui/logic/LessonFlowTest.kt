package com.teachme.sdui.logic

import com.teachme.sdui.Fixtures
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.ParseResult
import com.teachme.sdui.model.QuizSingle
import com.teachme.sdui.model.Unavailable
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LessonFlowTest {
    private fun doc(name: String): LessonDocument =
        (LessonParser.parse(Fixtures.text(name)) as ParseResult.Usable).document

    private fun sectionsOf(name: String) = SectionBuilder.build(doc(name))

    private val none: ExerciseLookup = { ExerciseState() }

    @Test
    fun `sections split at level 2 headings with an introduction first`() {
        val s = sectionsOf("lesson-rust-ownership.json")
        assertTrue(s.sections.first().isIntro)
        assertEquals((1..s.numberedCount).toList(), s.sections.mapNotNull { it.number })
        assertTrue(s.showsProgress)
        s.sections
            .drop(
                1,
            ).forEach { assertTrue(it.items.first() is Heading && (it.items.first() as Heading).level == 2) }
        assertEquals(doc("lesson-rust-ownership.json").blocks.size, s.sections.sumOf { it.items.size })
    }

    @Test
    fun `a lesson without level 2 headings is one section with no progress`() {
        val text = """{"blocks":[{"id":"h","type":"heading","level":1,"text":"T"},
            {"id":"p","type":"paragraph","text":"x"}]}"""
        val s = SectionBuilder.build((LessonParser.parse(text) as ParseResult.Usable).document)
        assertEquals(1, s.sections.size)
        assertFalse(s.showsProgress)
    }

    @Test
    fun `missing component is flattened into its fallback and its exercises count`() {
        val text =
            """{"blocks":[{"id":"h","type":"heading","level":2,"text":"S"},
               {"id":"m","type":"missing_component","wanted":"timeline","purpose":"x","fallback":[
                  {"id":"fp","type":"paragraph","text":"ciao"},
                  {"id":"fq","type":"quiz_single","question":"?","options":[{"id":"a","text":"A","feedback":"f"},{"id":"b","text":"B","feedback":"f"}],"correct":"a"}]}]}"""
        val s = SectionBuilder.build((LessonParser.parse(text) as ParseResult.Usable).document)
        assertEquals(
            listOf("h", "fp", "fq"),
            s.sections
                .single()
                .items
                .map { it.id },
        )
        assertEquals(
            listOf("fq"),
            s.sections
                .single()
                .requiredExercises
                .map { it.id },
        )
    }

    @Test
    fun `consecutive neutral rows fuse into one and do not count as exercises`() {
        val text =
            """{"blocks":[{"id":"h","type":"heading","level":2,"text":"S"},{"id":"u1","type":"zzz"},{"id":"u2","type":"yyy"},{"id":"p","type":"paragraph","text":"x"}]}"""
        val s = SectionBuilder.build((LessonParser.parse(text) as ParseResult.Usable).document).sections.single()
        assertEquals(listOf("h", "u1", "p"), s.items.map { it.id })
        assertTrue(s.items[1] is Unavailable)
        assertTrue(s.requiredExercises.isEmpty())
        assertTrue(LessonProgress.isUnlocked(s, none))
    }

    @Test
    fun `a section with only unavailable blocks unlocks by itself`() {
        val text = """{"blocks":[{"id":"h","type":"heading","level":2,"text":"S"},{"id":"u","type":"zzz"},
            {"id":"p","type":"paragraph","text":"x"}]}"""
        val s = SectionBuilder.build((LessonParser.parse(text) as ParseResult.Usable).document)
        assertTrue(s.sections.all { LessonProgress.isUnlocked(it, none) })
    }

    @Test
    fun `continue is locked until every required exercise is answered, right or wrong`() {
        val s = sectionsOf("lesson-rust-ownership.json")
        val withExercise = s.sections.first { it.requiredExercises.isNotEmpty() }
        val required = withExercise.requiredExercises
        assertFalse(LessonProgress.isUnlocked(withExercise, none))
        assertEquals(required.map { it.id }, LessonProgress.missing(withExercise, none).map { it.id })
        val wrong =
            ExerciseState(attempts = 1, checked = true, outcome = Outcome.Incorrect, firstOutcome = Outcome.Incorrect)
        assertTrue(LessonProgress.isUnlocked(withExercise) { wrong })
        // Selecting is not answering.
        val onlySelected: ExerciseLookup = { ExerciseState(selection = listOf("a")) }
        assertFalse(LessonProgress.isUnlocked(withExercise, onlySelected))
    }

    @Test
    fun `optional exercises never lock`() {
        val s = sectionsOf("lesson-rust-ownership.json")
        val optional = s.sections.flatMap { it.exercises }.filter { !it.required }
        assertTrue(optional.isNotEmpty())
        s.sections.forEach { sec ->
            val missing = LessonProgress.missing(sec, none)
            assertTrue(missing.none { !it.required })
        }
    }

    @Test
    fun `flashcard and open answer are done by rating and by submission, not by looking`() {
        assertFalse(ExerciseState(revealed = true).done)
        assertTrue(ExerciseState(revealed = true, rating = Rating.Hard).done)
        assertFalse(ExerciseState(texts = mapOf("answer" to "x")).done)
        assertTrue(ExerciseState(submitted = true).done)
    }

    @Test
    fun `progress follows the formula and ends at 1 on completion`() {
        val s = sectionsOf("lesson-rust-ownership.json")
        val intro = s.sections.first()
        assertEquals(0f, LessonProgress.value(s, intro, none, completed = false), 0.0001f)
        val second = s.sections[1]
        val n = s.numberedCount
        assertEquals(0f, LessonProgress.value(s, second, none, false), 0.0001f)
        val required = second.requiredExercises
        if (required.isNotEmpty()) {
            val half: ExerciseLookup = { id ->
                if (id ==
                    required.first().id
                ) {
                    ExerciseState(attempts = 1)
                } else {
                    ExerciseState()
                }
            }
            assertEquals((0f + 1f / required.size) / n, LessonProgress.value(s, second, half, false), 0.0001f)
        }
        val third = s.sections[2]
        assertEquals(1f / n, LessonProgress.value(s, third, none, false), 0.0001f)
        assertEquals(1f, LessonProgress.value(s, third, none, true), 0.0001f)
    }

    @Test
    fun `exercises of passed sections are locked`() {
        assertFalse(LessonProgress.isLocked(0, 1))
        assertTrue(LessonProgress.isLocked(0, 2))
        assertFalse(LessonProgress.isLocked(1, 2))
    }

    @Test
    fun `context index lists only terms and sources already met and never leaks answers`() {
        val d = doc("lesson-rust-ownership.json")
        val s = SectionBuilder.build(d)
        val introOnly = LessonIndex.encounteredTerms(d, s.sections.take(1))
        val all = LessonIndex.encounteredTerms(d, s.sections)
        assertTrue(all.size >= introOnly.size)
        assertTrue(all.all { d.glossaryEntry(it) != null })
        assertEquals(all.distinct(), all)
        assertTrue(LessonIndex.citedSources(d, s.sections).isNotEmpty())
        assertTrue(LessonIndex.citedSources(d, s.sections.take(1)).size <= LessonIndex.citedSources(d, s.sections).size)
    }

    @Test
    fun `summary judges first attempts`() {
        val d = doc("lesson-rust-ownership.json")
        val s = SectionBuilder.build(d)
        val graded = s.sections.flatMap { it.exercises }.filter { it is QuizSingle }
        val first = graded.first()
        val second = graded[1]
        val states =
            mapOf(
                first.id to
                    ExerciseState(
                        attempts = 1,
                        checked = true,
                        outcome = Outcome.Correct,
                        firstOutcome = Outcome.Correct,
                    ),
                second.id to
                    ExerciseState(
                        attempts = 1,
                        checked = true,
                        outcome = Outcome.Incorrect,
                        firstOutcome = Outcome.Incorrect,
                    ),
            )
        val summary = SummaryBuilder.build(s, states = { states[it] ?: ExerciseState() })
        assertEquals(listOf(second.id), summary.toReview.map { it.blockId })
        assertTrue(
            summary.toReview
                .single()
                .question
                .isNotBlank(),
        )
        assertFalse(
            summary.toReview
                .single()
                .question
                .contains("**"),
        )
        assertTrue(summary.sections.any { it.result == SectionResult.Review })
        assertTrue(summary.sections.any { it.result == SectionResult.CorrectFirstTry })
    }

    @Test
    fun `summary counts a correct second attempt as such and ignores flashcards and open answers`() {
        val d = doc("lesson-git-staging.json")
        val s = SectionBuilder.build(d)
        val multi = s.sections.flatMap { it.exercises }.first { it.type == "quiz_multi" }
        val states =
            mapOf(
                multi.id to
                    ExerciseState(
                        attempts = 2,
                        checked = true,
                        outcome = Outcome.Correct,
                        firstOutcome = Outcome.Partial,
                    ),
            )
        val summary = SummaryBuilder.build(s, states = { states[it] ?: ExerciseState() })
        assertEquals(SectionResult.CorrectSecondTry, summary.sections.single().result)
        assertEquals(listOf(multi.id), summary.toReview.map { it.blockId })
        val noisy =
            SummaryBuilder.build(s, states = {
                if (it in
                    s.sections
                        .flatMap { sec ->
                            sec.exercises
                        }.filter { e -> e is Flashcard || e is OpenAnswer }
                        .map { e -> e.id }
                ) {
                    ExerciseState(submitted = true, rating = Rating.Good)
                } else {
                    ExerciseState()
                }
            })
        assertTrue(noisy.sections.isEmpty())
    }

    @Test
    fun `every exercise of every valid lesson belongs to exactly one section`() {
        for (name in Fixtures.validLessons) {
            val d = doc(name)
            val s = SectionBuilder.build(d)
            val ids = s.sections.flatMap { it.exercises }.map(Exercise::id)
            assertEquals(name, ids.size, ids.toSet().size)
        }
    }

    @Test
    fun `state of vanished blocks is pruned and reveal count clamped`() {
        val saved =
            LessonSavedState(
                revealedCount = 9,
                exercises = mapOf("a" to ExerciseState(attempts = 1), "gone" to ExerciseState(attempts = 1)),
                scrollBlockId = "b:gone",
            )
        val pruned = saved.prunedTo(setOf("a", "b"), sectionCount = 4)
        assertEquals(setOf("a"), pruned.exercises.keys)
        assertEquals(4, pruned.revealedCount)
        assertNull(pruned.scrollBlockId)
        // Rows that are not blocks stay valid anchors.
        assertEquals("objective", saved.copy(scrollBlockId = "objective").prunedTo(setOf("a"), 4).scrollBlockId)
        assertEquals("b:a", saved.copy(scrollBlockId = "b:a").prunedTo(setOf("a"), 4).scrollBlockId)
        assertEquals(1, saved.copy(revealedCount = 0).prunedTo(setOf("a"), 3).revealedCount)
    }

    @Test
    fun `saved state survives a round trip and bad data gives a fresh state`() {
        val saved =
            LessonSavedState(
                revealedCount = 3,
                exercises =
                    mapOf(
                        "q" to
                            ExerciseState(
                                selection = listOf("b"),
                                attempts = 1,
                                checked = true,
                                outcome = Outcome.Correct,
                                firstOutcome = Outcome.Correct,
                                firstCheckedAtMs = 42,
                            ),
                        "f" to ExerciseState(texts = mapOf("b1" to "spostato", "b2" to "x")),
                        "o" to ExerciseState(order = listOf("s3", "s1", "s2")),
                        "fc" to ExerciseState(revealed = true, rating = Rating.Hard),
                    ),
                openedTerms = listOf("ownership"),
                disclosures = mapOf("glossary" to true),
                scrollBlockId = "p4",
                scrollOffsetPx = 120,
                completed = true,
            )
        assertEquals(saved, LessonSavedState.decode(saved.encode()))
        assertEquals(LessonSavedState(), LessonSavedState.decode(null))
        assertEquals(LessonSavedState(), LessonSavedState.decode("{{{ broken"))
        assertEquals(
            LessonSavedState(revealedCount = 2),
            LessonSavedState.decode("""{"revealedCount":2,"future_field":1}"""),
        )
    }
}
