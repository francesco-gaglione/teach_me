package com.teachme.sdui.render

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.ExerciseCode
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

internal val ISSUE_CODE =
    ExerciseCode(
        "rust",
        "let mut s = String::from(\"ciao\");\nlet r = &s;\nprintln!(\"{}\", r);\ns.push('!');\nprintln!(\"{}\", s);",
    )

private const val ISSUE_QUESTION =
    "Il compilatore deve garantire che il prestito `r` resti valido per quale tratto di codice?"

private val CODE_QUIZ =
    QuizSingle(
        "cq1",
        code = ISSUE_CODE,
        question = ISSUE_QUESTION,
        options =
            listOf(
                ChoiceOption("a", "Fino all'ultimo `println!`", "No: `r` non viene piu usato dopo il primo."),
                ChoiceOption("b", "Fino al primo `println!`", "Esatto: e l'ultimo uso di `r`."),
                ChoiceOption("c", "Fino a `s.push('!')`", "Il prestito finisce prima."),
            ),
        correct = "b",
    )
private val CODE_MULTI =
    QuizMulti(
        "cm1",
        code = ISSUE_CODE,
        question = "Quali righe usano `s` mentre `r` e ancora viva?",
        options = listOf("a", "b", "c").map { ChoiceOption(it, "Opzione $it", "Spiegazione $it") },
        correct = listOf("a"),
    )
private val CODE_OPEN =
    OpenAnswer("co1", code = ISSUE_CODE, question = "Spiega perche questo programma compila.", maxChars = 600)
private val CODE_FLASH =
    Flashcard(
        "cf1",
        code = ISSUE_CODE,
        front = "Perche `s.push('!')` e ammesso qui?",
        back = "Perche `r` non e piu usato.",
    )
private val LONG_CODE =
    ExerciseCode("rust", (1..20).joinToString("\n") { "let valore_numero_$it = calcola($it, \"parametro lungo\");" })
private val LONG_CODE_QUIZ = CODE_QUIZ.copy(id = "cq2", code = LONG_CODE)

private val FLASH_REVEALED = ExerciseState(revealed = true)

@Composable
private fun CodeCards(
    dark: Boolean,
    states: Map<String, ExerciseState> = emptyMap(),
) = PreviewFrame(dark, states) {
    QuizSingleBlock(CODE_QUIZ)
    QuizMultiBlock(CODE_MULTI)
    OpenAnswerBlock(CODE_OPEN)
    FlashcardBlock(CODE_FLASH)
}

@Preview(name = "Exercise code, normal", showBackground = true, heightDp = 1900)
@Composable
private fun ExerciseCodeNormal() = CodeCards(dark = false)

@Preview(name = "Exercise code, right answer", showBackground = true, heightDp = 1900)
@Composable
private fun ExerciseCodeRight() = CodeCards(dark = false, states = mapOf("cq1" to RIGHT))

@Preview(name = "Exercise code, wrong answer", showBackground = true, heightDp = 1900)
@Composable
private fun ExerciseCodeWrong() = CodeCards(dark = false, states = mapOf("cq1" to WRONG))

@Preview(name = "Exercise code, dark", showBackground = true, heightDp = 1900)
@Composable
private fun ExerciseCodeDark() = CodeCards(dark = true, states = mapOf("cq1" to RIGHT, "cf1" to FLASH_REVEALED))

@Preview(name = "Exercise code, flashcard back", showBackground = true, heightDp = 700)
@Composable
private fun ExerciseCodeFlashcardBack() =
    PreviewFrame(dark = false, states = mapOf("cf1" to FLASH_REVEALED)) { FlashcardBlock(CODE_FLASH) }

@Preview(name = "Exercise code, 20 lines collapsed", showBackground = true, heightDp = 900)
@Composable
private fun ExerciseCodeLong() = PreviewFrame(dark = false) { QuizSingleBlock(LONG_CODE_QUIZ) }
