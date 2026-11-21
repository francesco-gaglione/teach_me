package com.teachme.feature.chat.markdown

/** Inline content of a paragraph, list item or heading. Pure data: styling is decided by the renderer. */
sealed interface MdInline {
    data class Text(
        val text: String,
        val bold: Boolean = false,
        val italic: Boolean = false,
        val code: Boolean = false,
    ) : MdInline

    /** A link that passed validation (https only, parsable URL). */
    data class Link(
        val label: String,
        val url: String,
        val bold: Boolean = false,
        val italic: Boolean = false,
    ) : MdInline

    /** Hard line break (two trailing spaces or a trailing backslash). */
    data object LineBreak : MdInline
}

/** One list entry. [level] is 0 or 1 (deeper levels are flattened); [ordinal] is null for bullets. */
data class MdListItem(
    val level: Int,
    val ordinal: Int?,
    val inlines: List<MdInline>,
)

sealed interface MdBlock {
    data class Paragraph(
        val inlines: List<MdInline>,
    ) : MdBlock

    /** `#`..`####`: shown as a bold paragraph, never bigger (chat.md section 5). */
    data class Heading(
        val inlines: List<MdInline>,
    ) : MdBlock

    /** Fenced code, or a table rendered as code ([table] true, no language). [closed] false while streaming. */
    data class Code(
        val language: String?,
        val code: String,
        val closed: Boolean,
        val table: Boolean = false,
    ) : MdBlock

    data class ListBlock(
        val items: List<MdListItem>,
    ) : MdBlock

    data object Rule : MdBlock
}
