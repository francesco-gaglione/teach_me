package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Outcome
import com.teachme.sdui.logic.Rating
import com.teachme.sdui.model.AskAgent
import com.teachme.sdui.model.Blank
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.Checklist
import com.teachme.sdui.model.ChecklistItem
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.CodeBlock
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.DiagramEdge
import com.teachme.sdui.model.DiagramNode
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.GlossaryEntry
import com.teachme.sdui.model.GlossaryTermBlock
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.LessonLink
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderStep
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.OutcomeFeedback
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.model.PrimarySource
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import com.teachme.sdui.model.SourceEntry

private val SAMPLE_DOCUMENT =
    LessonDocument(
        catalogVersion = 1,
        title = "Ownership e move",
        objective = "Saper prevedere se una variabile e ancora utilizzabile.",
        estimatedMinutes = 8,
        glossary =
            listOf(
                GlossaryEntry(
                    "ownership",
                    "ownership",
                    "La regola per cui ogni valore ha **un solo** proprietario.",
                    listOf("proprieta intellettuale"),
                ),
            ),
        sources =
            listOf(
                SourceEntry(
                    "rust_book",
                    null,
                    "The Rust Book, 4.1 What is Ownership?",
                    "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html",
                ),
            ),
        blocks = listOf(Heading("h", level = 1, text = "Ownership e move")),
    )

private val NO_HOST =
    object : SduiHost {
        override fun openTerm(key: String) = Unit

        override fun openSource(key: String) = Unit

        override fun openUrl(url: String) = Unit

        override fun askAgent(promptHint: String) = Unit

        override fun openLesson(lessonId: String) = Unit

        override fun lessonLinkTarget(lessonId: String) = null

        override fun copyText(text: String) = Unit
    }

private class FixedStates(
    private val states: Map<String, ExerciseState>,
    private val locked: Boolean = false,
) : ExerciseStates {
    private val live = states.toMutableMap()

    override fun get(blockId: String) = live[blockId] ?: ExerciseState()

    override fun update(
        blockId: String,
        transform: (ExerciseState) -> ExerciseState,
    ) {
        live[blockId] = transform(get(blockId))
    }

    override fun isLocked(blockId: String) = locked
}

@Composable
internal fun PreviewFrame(
    dark: Boolean,
    states: Map<String, ExerciseState> = emptyMap(),
    keyboard: Boolean = false,
    content: @Composable () -> Unit,
) {
    TeachTheme(darkTheme = dark) {
        val env = SduiEnvironment(SAMPLE_DOCUMENT, NO_HOST, FixedStates(states), keyboard)
        CompositionLocalProvider(LocalSdui provides env) {
            Column(
                Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGap),
            ) { content() }
        }
    }
}

private val QUIZ =
    QuizSingle(
        "q1",
        question = "Dopo `let b = a;`, si puo ancora usare [[ownership|`a`]]?",
        options =
            listOf(
                ChoiceOption("a", "Si, perche `a` e ancora nello scope", "Sembra ragionevole, ma lo scope non decide."),
                ChoiceOption(
                    "b",
                    "No, perche il valore e stato spostato",
                    "Con `let b = a;` la proprieta passa a `b`.",
                ),
                ChoiceOption("c", "Si, ma solo per leggerlo", "Dopo il move non si puo leggere."),
            ),
        correct = "b",
    )
private val MULTI =
    QuizMulti(
        "m1",
        question = "Quali operazioni spostano il valore?",
        options = listOf("a", "b", "c", "d").map { ChoiceOption(it, "Opzione $it", "Spiegazione $it") },
        correct = listOf("a", "c"),
        required = false,
    )
private val FILL =
    FillBlank(
        "f1",
        text = "Dopo `let b = a;` il valore e stato {{x}} a `b`, quindi `a` non e {{y}}.",
        blanks = listOf(Blank("x", listOf("spostato")), Blank("y", listOf("valido"))),
        feedback = OutcomeFeedback("Esatto: e un **move**.", "Chiediti a chi appartiene il valore."),
    )
private val ORDER =
    OrderSteps(
        "o1",
        prompt = "Ordina i passi per registrare un commit.",
        steps =
            listOf(
                "Modifica il file",
                "Aggiungi con `git add`",
                "Verifica con `git diff --staged`",
                "Registra con `git commit`",
            ).mapIndexed {
                i,
                t,
                ->
                OrderStep("s$i", t)
            },
        feedback = OutcomeFeedback("Giusto.", "Il commit registra la staging area."),
    )
private val FLASH =
    Flashcard(
        "fc1",
        front = "Che cosa succede ad `a` dopo `let b = a;`?",
        back = "Viene **spostata**: `a` non e piu valida.",
    )
private val OPEN = OpenAnswer("oa1", question = "Spiega con parole tue perche `a` non e piu valida.", maxChars = 600)

internal val RIGHT =
    ExerciseState(
        selection = listOf("b"),
        attempts = 1,
        checked = true,
        outcome = Outcome.Correct,
        firstOutcome = Outcome.Correct,
    )
internal val WRONG =
    ExerciseState(
        selection = listOf("a"),
        attempts = 1,
        checked = true,
        outcome = Outcome.Incorrect,
        firstOutcome = Outcome.Incorrect,
    )

@Composable
private fun TextBlocksSample(dark: Boolean) =
    PreviewFrame(dark) {
        HeadingBlock(Heading("h", level = 1, text = "Ownership e move"))
        HeadingBlock(Heading("h2", level = 2, text = "Quando `a` smette di essere valida"))
        ParagraphBlock(
            Paragraph(
                "p",
                text =
                    "In Rust ogni valore ha un **proprietario**. Quando [[ownership|lo sposti]] la prima " +
                        "non e piu valida[^rust_book]. Vedi [la guida](https://example.com).",
            ),
        )
        CalloutBlock(
            Callout(
                "c1",
                variant = CalloutVariant.Warning,
                title = "Errore comune",
                text = "`b = a` non copia: sposta.",
            ),
        )
        CalloutBlock(Callout("c2", variant = CalloutVariant.Tip, text = "Un suggerimento."))
        CalloutBlock(Callout("c3", variant = CalloutVariant.Info, text = "Un contesto laterale."))
        GlossaryTermBlockView(GlossaryTermBlock("g", key = "ownership"))
        UnavailableBlockView()
    }

@Preview(name = "Text blocks light", showBackground = true, heightDp = 1500)
@Composable
private fun TextBlocksLight() = TextBlocksSample(false)

@Preview(name = "Text blocks dark", showBackground = true, heightDp = 1500)
@Composable
private fun TextBlocksDark() = TextBlocksSample(true)

@Composable
private fun CodeSample(dark: Boolean) =
    PreviewFrame(dark) {
        CodeBlockView(
            CodeBlock(
                "code",
                language = "rust",
                code = "fn main() {\n    let a = String::from(\"ciao\");\n    let b = a;\n    // println!(\"{a}\");\n}",
                caption = "Il compilatore rifiuta la riga 4 se togli il commento.",
                highlightLines = listOf(3),
            ),
        )
        CodeBlockView(CodeBlock("code2", language = "text", code = (1..20).joinToString("\n") { "riga $it" }))
    }

@Preview(name = "Code light", showBackground = true, heightDp = 900)
@Composable
private fun CodeLight() = CodeSample(false)

@Preview(name = "Code dark", showBackground = true, heightDp = 900)
@Composable
private fun CodeDark() = CodeSample(true)

@Preview(name = "Quiz idle", showBackground = true, heightDp = 700)
@Composable
private fun QuizIdle() = PreviewFrame(false, keyboard = true) { QuizSingleBlock(QUIZ) }

@Preview(name = "Quiz right", showBackground = true, heightDp = 900)
@Composable
private fun QuizRight() = PreviewFrame(false, mapOf("q1" to RIGHT)) { QuizSingleBlock(QUIZ) }

@Preview(name = "Quiz wrong", showBackground = true, heightDp = 1000)
@Composable
private fun QuizWrong() = PreviewFrame(false, mapOf("q1" to WRONG)) { QuizSingleBlock(QUIZ) }

@Preview(name = "Quiz wrong dark", showBackground = true, heightDp = 1000)
@Composable
private fun QuizWrongDark() = PreviewFrame(true, mapOf("q1" to WRONG)) { QuizSingleBlock(QUIZ) }

@Preview(name = "Quiz font 2.0", showBackground = true, heightDp = 1400, fontScale = 2f)
@Composable
private fun QuizLargeFont() = PreviewFrame(false) { QuizSingleBlock(QUIZ) }

@Preview(name = "Multi idle and partial", showBackground = true, heightDp = 1500)
@Composable
private fun MultiStates() =
    PreviewFrame(
        false,
        mapOf(
            "m1" to
                ExerciseState(
                    selection = listOf("a", "b"),
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Partial,
                    firstOutcome = Outcome.Partial,
                ),
        ),
    ) {
        QuizMultiBlock(MULTI)
    }

@Preview(name = "Multi right dark", showBackground = true, heightDp = 1300)
@Composable
private fun MultiRightDark() =
    PreviewFrame(
        true,
        mapOf(
            "m1" to
                ExerciseState(
                    selection = listOf("a", "c"),
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Correct,
                    firstOutcome = Outcome.Correct,
                ),
        ),
    ) {
        QuizMultiBlock(MULTI)
    }

@Preview(name = "Fill idle", showBackground = true, heightDp = 500)
@Composable
private fun FillIdle() = PreviewFrame(false) { FillBlankBlock(FILL) }

@Preview(name = "Fill right", showBackground = true, heightDp = 700)
@Composable
private fun FillRight() =
    PreviewFrame(
        false,
        mapOf(
            "f1" to
                ExerciseState(
                    texts = mapOf("x" to "spostato", "y" to "valido"),
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Correct,
                    firstOutcome = Outcome.Correct,
                    blankResults =
                        mapOf(
                            "x" to true,
                            "y" to true,
                        ),
                ),
        ),
    ) {
        FillBlankBlock(FILL)
    }

@Preview(name = "Fill wrong dark", showBackground = true, heightDp = 800)
@Composable
private fun FillWrongDark() =
    PreviewFrame(
        true,
        mapOf(
            "f1" to
                ExerciseState(
                    texts = mapOf("x" to "spostato", "y" to "ok"),
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Partial,
                    firstOutcome = Outcome.Partial,
                    blankResults =
                        mapOf(
                            "x" to true,
                            "y" to false,
                        ),
                ),
        ),
    ) {
        FillBlankBlock(FILL)
    }

@Preview(name = "Fill font 2.0 (list mode)", showBackground = true, heightDp = 1200, fontScale = 2f)
@Composable
private fun FillLargeFont() = PreviewFrame(false) { FillBlankBlock(FILL) }

@Preview(name = "Order idle", showBackground = true, heightDp = 700)
@Composable
private fun OrderIdle() = PreviewFrame(false) { OrderStepsBlock(ORDER) }

@Preview(name = "Order wrong", showBackground = true, heightDp = 1000)
@Composable
private fun OrderWrong() =
    PreviewFrame(
        false,
        mapOf(
            "o1" to
                ExerciseState(
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Incorrect,
                    firstOutcome = Outcome.Incorrect,
                ),
        ),
    ) {
        OrderStepsBlock(ORDER)
    }

@Preview(name = "Order right dark", showBackground = true, heightDp = 1000)
@Composable
private fun OrderRightDark() =
    PreviewFrame(
        true,
        mapOf(
            "o1" to
                ExerciseState(
                    order =
                        ORDER.steps.map {
                            it.id
                        },
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Correct,
                    firstOutcome = Outcome.Correct,
                ),
        ),
    ) {
        OrderStepsBlock(ORDER)
    }

@Preview(name = "Flashcard states", showBackground = true, heightDp = 1300)
@Composable
private fun FlashcardStates() =
    PreviewFrame(false) {
        FlashcardBlock(FLASH)
        FlashcardBlock(Flashcard("fc2", front = "Fronte", back = "Retro", required = false))
    }

@Preview(name = "Flashcard revealed dark", showBackground = true, heightDp = 700)
@Composable
private fun FlashcardRevealedDark() =
    PreviewFrame(true, mapOf("fc1" to ExerciseState(revealed = true)), keyboard = true) {
        FlashcardBlock(FLASH)
    }

@Preview(name = "Flashcard rated", showBackground = true, heightDp = 600)
@Composable
private fun FlashcardRated() =
    PreviewFrame(false, mapOf("fc1" to ExerciseState(revealed = true, rating = Rating.Hard))) {
        FlashcardBlock(FLASH)
    }

private val OPEN_ANSWER_TEXT = mapOf("answer" to "Perche il valore e passato a `b` e `a` non e piu valida.")
private const val OPEN_FEEDBACK =
    "Giusto che il valore si sposta in `b`; manca *perche* `a` non e piu valida: il proprietario e uno solo."

private fun openStates(vararg states: Pair<String, ExerciseState>) = mapOf(*states)

private fun ExerciseState.openSent() = copy(texts = OPEN_ANSWER_TEXT, submitted = true)

/** The five states of `open_answer` after "Invia": evaluating, graded (three verdicts), registered (three origins). */
@Composable
private fun OpenAnswerAll(dark: Boolean) =
    PreviewFrame(
        dark,
        openStates(
            "oa2" to ExerciseState().openSent().copy(evaluating = true),
            "oa3" to
                ExerciseReducers.graded(
                    ExerciseState().openSent(),
                    Outcome.Correct,
                    "Corretto: hai spiegato il *move*.",
                ),
            "oa4" to ExerciseReducers.graded(ExerciseState().openSent(), Outcome.Partial, OPEN_FEEDBACK),
            "oa5" to ExerciseReducers.graded(ExerciseState().openSent(), Outcome.Incorrect, OPEN_FEEDBACK),
            "oa6" to ExerciseState().openSent().copy(serverPending = true),
            "oa7" to ExerciseState().openSent(),
            "oa8" to ExerciseState().openSent().copy(serverPending = true, reevalFailed = true),
            "oa9" to ExerciseState().openSent().copy(notEvaluable = true),
        ),
    ) {
        OpenAnswerBlock(OPEN)
        for (i in 2..9) OpenAnswerBlock(OPEN.copy(id = "oa$i", required = false))
    }

@Preview(name = "Open answer states", showBackground = true, heightDp = 4200)
@Composable
private fun OpenAnswerStates() = OpenAnswerAll(false)

@Preview(name = "Open answer states dark", showBackground = true, heightDp = 4200)
@Composable
private fun OpenAnswerStatesDark() = OpenAnswerAll(true)

@Preview(name = "Open answer font 2.0", showBackground = true, heightDp = 3000, widthDp = 360, fontScale = 2f)
@Composable
private fun OpenAnswerLarge() =
    PreviewFrame(
        false,
        openStates("oa2" to ExerciseReducers.graded(ExerciseState().openSent(), Outcome.Partial, OPEN_FEEDBACK)),
    ) {
        OpenAnswerBlock(OPEN.copy(id = "oa2"))
    }

@Preview(name = "Open answer dark", showBackground = true, heightDp = 600)
@Composable
private fun OpenAnswerDark() = PreviewFrame(true) { OpenAnswerBlock(OPEN) }

@Preview(name = "Closed lesson, unanswered exercise", showBackground = true, heightDp = 800)
@Composable
private fun ClosedLessonPreview() =
    TeachTheme(darkTheme = false) {
        val env = SduiEnvironment(SAMPLE_DOCUMENT, NO_HOST, FixedStates(emptyMap()), readOnly = true)
        androidx.compose.runtime.CompositionLocalProvider(LocalSdui provides env) {
            Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
                DefaultBlockRegistry.Render(OPEN)
            }
        }
    }

@Preview(name = "Checklist", showBackground = true, heightDp = 600)
@Composable
private fun ChecklistStates() =
    PreviewFrame(false, mapOf("cl" to ExerciseState(selection = listOf("i1")))) {
        ChecklistBlock(
            Checklist(
                "cl",
                title = "Prova tu",
                items =
                    listOf(
                        ChecklistItem("i1", "Scrivi un esempio con `let b = a;`"),
                        ChecklistItem("i2", "Compilalo e leggi l'errore"),
                    ),
            ),
        )
    }

@Composable
private fun NavigationSample(dark: Boolean) =
    PreviewFrame(dark) {
        PrimarySourceBlock(
            PrimarySource(
                "ps",
                source = "rust_book",
                why = "Spiega l'ownership con stack e heap, utile per capire perche `String` si sposta.",
            ),
        )
        LessonLinkBlock(LessonLink("ll", lessonId = "x", label = "Continua con il borrowing"))
        AskAgentBlock(AskAgent("aa", promptHint = "Non ho capito perche non posso piu usare `a`."))
    }

@Preview(name = "Navigation light", showBackground = true, heightDp = 600)
@Composable
private fun NavigationLight() = NavigationSample(false)

@Preview(name = "Navigation dark", showBackground = true, heightDp = 600)
@Composable
private fun NavigationDark() = NavigationSample(true)

private val DIAGRAM_TEXT_ONLY =
    Diagram(
        id = "fig",
        layout = "cycle",
        title = "Il ciclo del metodo scientifico",
        nodes = listOf("Osservazione", "Ipotesi", "Esperimento", "Analisi").map { DiagramNode(it.lowercase(), it) },
        caption = "Dopo l'analisi non c'e un traguardo: i dati portano a una **nuova osservazione**.",
        alt =
            "Quattro tappe disposte in cerchio, collegate da frecce nello stesso verso: osservazione, ipotesi, " +
                "esperimento, analisi dei dati. Dall'analisi dei dati una freccia torna all'osservazione.",
    )

/** Unknown layout (a newer catalog) and no title: the text version is complete, not an error. */
private val DIAGRAM_UNKNOWN_LAYOUT =
    DIAGRAM_TEXT_ONLY.copy(
        id = "fig2",
        layout = "matrix",
        title = null,
        edges = listOf(DiagramEdge("osservazione", "ipotesi")),
    )

@Composable
private fun DiagramSample(dark: Boolean) =
    PreviewFrame(dark) {
        DiagramBlockView(DIAGRAM_TEXT_ONLY)
        DiagramBlockView(DIAGRAM_UNKNOWN_LAYOUT)
    }

@Preview(name = "Diagram text fallback light", showBackground = true, heightDp = 700)
@Composable
private fun DiagramFallbackLight() = DiagramSample(false)

@Preview(name = "Diagram text fallback dark", showBackground = true, heightDp = 700)
@Composable
private fun DiagramFallbackDark() = DiagramSample(true)

@Preview(name = "Diagram text fallback font 2.0", showBackground = true, heightDp = 1300, fontScale = 2f)
@Composable
private fun DiagramFallbackLarge() = DiagramSample(false)
