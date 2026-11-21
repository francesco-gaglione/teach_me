package com.teachme.sdui.render

import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.Hyphens

/** The language a block is written in: its own `lang`, or the language of the Topic when it has none (ADR 0019). */
internal fun effectiveLang(
    blockLang: String?,
    contentLanguage: String,
): String = blockLang?.takeIf { it.isNotBlank() } ?: contentLanguage

private fun String.primaryLanguage(): String = lowercase().substringBefore('-')

/** True when [lang] is the language of the Topic (same primary subtag), false for a quotation in another one. */
internal fun isTopicLanguage(
    lang: String,
    contentLanguage: String,
): Boolean = lang.primaryLanguage() == contentLanguage.primaryLanguage()

/** Automatic hyphenation only for text in the language of the Topic: foreign quotations are never broken. */
internal fun hyphensFor(
    lang: String,
    contentLanguage: String,
): Hyphens = if (isTopicLanguage(lang, contentLanguage)) Hyphens.Auto else Hyphens.None

internal fun localesFor(lang: String): LocaleList =
    try {
        LocaleList(lang)
    } catch (_: RuntimeException) {
        LocaleList.current
    }
