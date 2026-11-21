package com.teachme.feature.lesson

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource

/** Italian names of the block types, for "Vai al blocco: ..."; the key `null` is the fallback for an unknown type. */
@Composable
internal fun blockKindNames(): Map<String?, String> =
    mapOf(
        "heading" to stringResource(R.string.lesson_block_kind_heading),
        "paragraph" to stringResource(R.string.lesson_block_kind_paragraph),
        "callout" to stringResource(R.string.lesson_block_kind_callout),
        "code" to stringResource(R.string.lesson_block_kind_code),
        "glossary_term" to stringResource(R.string.lesson_block_kind_glossary_term),
        "quiz_single" to stringResource(R.string.lesson_block_kind_quiz_single),
        "quiz_multi" to stringResource(R.string.lesson_block_kind_quiz_multi),
        "fill_blank" to stringResource(R.string.lesson_block_kind_fill_blank),
        "order_steps" to stringResource(R.string.lesson_block_kind_order_steps),
        "flashcard" to stringResource(R.string.lesson_block_kind_flashcard),
        "open_answer" to stringResource(R.string.lesson_block_kind_open_answer),
        "checklist" to stringResource(R.string.lesson_block_kind_checklist),
        "primary_source" to stringResource(R.string.lesson_block_kind_primary_source),
        "lesson_link" to stringResource(R.string.lesson_block_kind_lesson_link),
        "ask_agent" to stringResource(R.string.lesson_block_kind_ask_agent),
        "diagram" to stringResource(R.string.lesson_block_kind_diagram),
        null to stringResource(R.string.lesson_block_kind_other),
    )
