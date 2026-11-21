package com.teachme.feature.lesson

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.CardVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachErrorState
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachSegmentedSwitch
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.LoadError
import com.teachme.core.model.RecordKind
import com.teachme.core.model.RecordStatus
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.render.ProvideStandaloneSdui
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.time.Instant
import com.teachme.core.ui.R as UiR

private const val GLOSSARY_SEARCH_THRESHOLD = 20
private const val GLOSSARY_LETTER_HEADERS_THRESHOLD = 12
private const val RECORD_HIGHLIGHT_MS = 1_500L
private val GROUP_WIDTH_EXPANDED = 912.dp

/** Everything "Cosa sai" can ask of its owner. */
@Immutable
class ProgressTabActions(
    val onSelectSection: (ProgressSection) -> Unit,
    val onRetryRecords: () -> Unit,
    val onRetryGlossary: () -> Unit,
    val onOpenLesson: (String) -> Unit,
    val onGoToLessons: () -> Unit,
    val onOpenSettings: () -> Unit,
)

/**
 * The tab "Cosa sai" (docs/design/screens/progress.md): a switch between the register of what was demonstrated and the
 * glossary of the words the learner can use. [originOf] gives the lesson number and title of a lesson id when the list of
 * lessons knows it; [hasUnfinishedLesson] and [allCompleted] shape the empty register.
 */
@Composable
fun ProgressTab(
    state: ProgressUiState,
    actions: ProgressTabActions,
    originOf: (String?) -> LessonOrigin?,
    hasUnfinishedLesson: Boolean,
    allLessonsCompleted: Boolean,
    modifier: Modifier = Modifier,
    header: (@Composable () -> Unit)? = null,
) {
    val registerScroll = rememberScrollState()
    val glossaryScroll = rememberScrollState()
    val scroll = if (state.section == ProgressSection.Register) registerScroll else glossaryScroll
    BoxWithConstraints(modifier.fillMaxSize()) {
        val margin = screenMargin(currentWindowWidthClass())
        val layout = TeachTheme.layout
        val main = minOf(layout.readingMaxWidth, maxWidth - margin * 2)
        val group = if (maxWidth >= layout.contextPaneBreakpoint) GROUP_WIDTH_EXPANDED else main
        val start = ((maxWidth - group) / 2).coerceAtLeast(margin)
        Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
            header?.invoke()
            Column(
                Modifier
                    .padding(
                        start = start,
                        top = TeachTheme.spacing.md,
                        bottom = TeachTheme.spacing.xl,
                    ).width(main),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
            ) {
                TeachSegmentedSwitch(
                    labels =
                        listOf(
                            stringResource(R.string.progress_switch_register),
                            stringResource(R.string.progress_switch_glossary),
                        ),
                    selectedIndex = state.section.ordinal,
                    onSelect = { actions.onSelectSection(ProgressSection.entries[it]) },
                )
                key(state.section) {
                    when (state.section) {
                        ProgressSection.Register ->
                            RegisterView(state, actions, originOf, hasUnfinishedLesson, allLessonsCompleted)
                        ProgressSection.Glossary -> GlossaryView(state, actions, originOf)
                    }
                }
            }
        }
        UpdateAnnouncer(state)
    }
}

/** "Registro aggiornato." / "Glossario aggiornato.": polite, when a closing job succeeded while the tab is open. */
@Composable
private fun UpdateAnnouncer(state: ProgressUiState) {
    if (state.updatedSignal == 0) return
    val text =
        stringResource(
            if (state.section ==
                ProgressSection.Register
            ) {
                R.string.progress_updated_announce
            } else {
                R.string.progress_glossary_updated_announce
            },
        )
    key(state.updatedSignal) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

@Composable
private fun Skeleton(cards: Int) {
    val loading = stringResource(R.string.progress_loading)
    Column(
        Modifier.semantics(mergeDescendants = true) { stateDescription = loading },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        repeat(cards) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(
                        SKELETON_HEIGHT,
                    ).background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md),
            )
        }
    }
}

private val SKELETON_HEIGHT = 120.dp

@Composable
private fun SectionHeader(
    title: String,
    intro: String,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(title, Modifier.semantics { heading() }, style = TeachTheme.typography.ui.title2)
        TeachText(intro, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
    }
}

@Composable
private fun StaleLine() {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachIcon(TeachIcons.CloudOff, null, tint = TeachTheme.colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
        TeachText(
            stringResource(R.string.progress_stale),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun LoadFailed(
    title: String,
    error: LoadError,
    onRetry: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    TeachErrorState(
        title = title,
        body =
            stringResource(
                when (error) {
                    // A 401 gets no message of its own (login.md): the sign-in opens, the retry reloads.
                    LoadError.Network, LoadError.Unauthorized -> UiR.string.ui_load_network_body
                    LoadError.Server -> UiR.string.ui_load_server_body
                },
            ),
        primaryLabel = stringResource(R.string.progress_retry),
        onPrimary = onRetry,
    )
}

// --- register ---------------------------------------------------------------------------------------------------------

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RegisterView(
    state: ProgressUiState,
    actions: ProgressTabActions,
    originOf: (String?) -> LessonOrigin?,
    hasUnfinishedLesson: Boolean,
    allLessonsCompleted: Boolean,
) {
    SectionHeader(stringResource(R.string.progress_register_title), stringResource(R.string.progress_register_intro))
    val list = state.records
    when {
        list.items == null && list.error != null ->
            LoadFailed(
                stringResource(R.string.progress_register_error),
                list.error,
                actions.onRetryRecords,
                actions.onOpenSettings,
            )
        list.items == null -> Skeleton(2)
        else -> {
            if (list.stale) StaleLine()
            val active = state.activeRecords
            val superseded = state.supersededRecords
            val scope = rememberCoroutineScope()
            var highlighted by remember { mutableStateOf<String?>(null) }
            val requesters = remember { mutableMapOf<String, BringIntoViewRequester>() }

            fun requesterOf(id: String) = requesters.getOrPut(id) { BringIntoViewRequester() }
            LaunchedEffect(highlighted) {
                if (highlighted != null) {
                    delay(RECORD_HIGHLIGHT_MS)
                    highlighted = null
                }
            }
            if (active.isEmpty()) {
                EmptyRegister(hasUnfinishedLesson, allLessonsCompleted, actions.onGoToLessons)
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGap)) {
                    active.forEach { record ->
                        RecordCard(
                            record,
                            Modifier.bringIntoViewRequester(requesterOf(record.id)),
                            origin = originOf(record.lessonId) ?: record.lessonId?.let { LessonOrigin(it, null, null) },
                            onOpenLesson = actions.onOpenLesson,
                            highlighted = highlighted == record.id,
                        )
                    }
                }
            }
            if (superseded.isNotEmpty()) {
                var open by rememberSaveable { mutableStateOf(false) }
                TeachDisclosure(
                    title = stringResource(R.string.progress_superseded_title),
                    expanded = open,
                    onToggle = { open = !open },
                ) {
                    Column(
                        Modifier.padding(top = TeachTheme.spacing.xs),
                        verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGap),
                    ) {
                        superseded.forEach { record ->
                            val successor =
                                state.records.items
                                    .orEmpty()
                                    .firstOrNull { it.id == record.supersededBy }
                            RecordCard(
                                record,
                                origin =
                                    originOf(record.lessonId) ?: record.lessonId?.let { LessonOrigin(it, null, null) },
                                onOpenLesson = actions.onOpenLesson,
                                successorAvailable = successor != null && successor.status == RecordStatus.Active,
                                onSeeSuccessor = {
                                    successor?.let {
                                        highlighted = it.id
                                        scope.launch { requesterOf(it.id).bringIntoView() }
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyRegister(
    hasUnfinishedLesson: Boolean,
    allLessonsCompleted: Boolean,
    onGoToLessons: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
        TeachText(stringResource(R.string.progress_register_empty_title), style = TeachTheme.typography.ui.title3)
        TeachText(stringResource(R.string.progress_register_empty_body), style = TeachTheme.typography.ui.bodyL)
        if (hasUnfinishedLesson) {
            TeachButton(stringResource(R.string.progress_go_lessons), onGoToLessons, variant = ButtonVariant.Secondary)
        }
        if (allLessonsCompleted) {
            TeachText(
                stringResource(R.string.progress_register_empty_note),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

// --- glossary ---------------------------------------------------------------------------------------------------------

@Composable
private fun GlossaryView(
    state: ProgressUiState,
    actions: ProgressTabActions,
    originOf: (String?) -> LessonOrigin?,
) {
    SectionHeader(stringResource(R.string.progress_glossary_title), stringResource(R.string.progress_glossary_intro))
    val list = state.glossary
    var opened by remember { mutableStateOf<GlossaryTerm?>(null) }
    when {
        list.items == null && list.error != null ->
            LoadFailed(
                stringResource(R.string.progress_glossary_error),
                list.error,
                actions.onRetryGlossary,
                actions.onOpenSettings,
            )
        list.items == null -> Skeleton(4)
        list.items.isEmpty() -> {
            if (list.stale) StaleLine()
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                TeachText(
                    stringResource(R.string.progress_glossary_empty_title),
                    style = TeachTheme.typography.ui.title3,
                )
                TeachText(stringResource(R.string.progress_glossary_empty_body), style = TeachTheme.typography.ui.bodyL)
            }
        }
        else -> {
            if (list.stale) StaleLine()
            TermList(list.items, onOpen = { opened = it })
        }
    }
    opened?.let { term ->
        val origin = term.lessonId?.let { originOf(it) ?: LessonOrigin(it, null, null) }
        TopicGlossarySheet(
            term = term,
            origin = origin,
            numberOf = { originOf(it)?.number },
            onOpenLesson = { id ->
                opened = null
                actions.onOpenLesson(id)
            },
            onDismiss = { opened = null },
        )
    }
}

@Composable
private fun TermList(
    terms: List<GlossaryTerm>,
    onOpen: (GlossaryTerm) -> Unit,
) {
    var query by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    val showSearch = terms.size > GLOSSARY_SEARCH_THRESHOLD
    val filtered =
        remember(
            terms,
            query.text,
            showSearch,
        ) { if (showSearch) terms.filter { matchesQuery(it, query.text) } else terms }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    if (showSearch) {
        TeachTextField(
            value = query,
            onValueChange = { query = it },
            label = stringResource(R.string.progress_search_label),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions =
                KeyboardActions(
                    onSearch = {
                        keyboard?.hide()
                        focus.clearFocus()
                    },
                ),
        )
        ResultsAnnouncer(query.text, filtered.size)
    }
    if (filtered.isEmpty()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            TeachText(
                stringResource(R.string.progress_search_none, query.text.trim()),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(stringResource(R.string.progress_search_clear), {
                query = TextFieldValue("")
            }, variant = ButtonVariant.Text)
        }
        return
    }
    val withHeaders = terms.size > GLOSSARY_LETTER_HEADERS_THRESHOLD
    // The backend order is kept: the initial letters are only labels on the runs of the same letter.
    val groups = if (withHeaders) filtered.groupConsecutive { glossaryInitial(it.term) } else listOf(null to filtered)
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        groups.forEach { (letter, items) ->
            if (letter != null) {
                TeachText(
                    letter,
                    Modifier.heightIn(min = LETTER_HEIGHT).semantics { heading() },
                    style = TeachTheme.typography.ui.label,
                    color = TeachTheme.colors.inkSoft,
                )
            }
            TeachCard(Modifier.fillMaxWidth(), CardVariant.Static) {
                Column {
                    items.forEachIndexed { i, term ->
                        if (i > 0) TeachRowDivider()
                        GlossaryRow(term, onClick = { onOpen(term) })
                    }
                }
            }
        }
    }
}

private val LETTER_HEIGHT = 32.dp

/** Groups the elements that follow each other and share a key; the order never changes. */
private fun <T, K> List<T>.groupConsecutive(key: (T) -> K): List<Pair<K, List<T>>> {
    val out = mutableListOf<Pair<K, MutableList<T>>>()
    for (item in this) {
        val k = key(item)
        if (out.lastOrNull()?.first == k) out.last().second += item else out += k to mutableListOf(item)
    }
    return out
}

/** The number of results after the filter, spoken after a pause (progress.md 4.4). */
@Composable
private fun ResultsAnnouncer(
    query: String,
    count: Int,
) {
    if (query.isBlank()) return
    var spoken by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(query, count) {
        spoken = null
        delay(RESULTS_DEBOUNCE_MS)
        spoken = count
    }
    val text = spoken?.let { pluralStringResource(R.plurals.progress_search_results, it, it) } ?: return
    Box(
        Modifier.size(1.dp).semantics {
            liveRegion = LiveRegionMode.Polite
            contentDescription = text
        },
    )
}

private const val RESULTS_DEBOUNCE_MS = 500L

@Composable
private fun GlossaryRow(
    term: GlossaryTerm,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val firstSentence = InlineMarkdown.plainText(term.definition) { null }.substringBefore(". ").trim()
    val description = stringResource(R.string.progress_term_row_a11y, term.term, firstSentence)
    ProvideStandaloneSdui {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = GLOSSARY_ROW_MIN_HEIGHT)
                .focusRing(state.focused, TeachTheme.shapes.md)
                .background(
                    if (state.pressed) colors.surfacePressed else androidx.compose.ui.graphics.Color.Transparent,
                ).semantics(mergeDescendants = true) { contentDescription = description }
                .clickable(interactionSource = state.source, indication = null, role = Role.Button, onClick = onClick)
                .padding(start = TeachTheme.spacing.md, top = TeachTheme.spacing.xs, bottom = TeachTheme.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            TeachIcon(TeachIcons.BookOpen, null, tint = colors.knowledge)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                TeachText(term.term, style = TeachTheme.typography.ui.title3, color = colors.ink)
                TeachText(
                    InlineMarkdown.plainText(term.definition) { null },
                    style = TeachTheme.typography.ui.body,
                    color = colors.inkSoft,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            TeachIcon(
                TeachIcons.ChevronRight,
                null,
                Modifier.padding(end = TeachTheme.spacing.xs),
                tint = colors.inkMuted,
            )
        }
    }
}

private val GLOSSARY_ROW_MIN_HEIGHT: Dp = 72.dp

// --- previews ----------------------------------------------------------------------------------------------------------

private fun record(
    id: String,
    kind: RecordKind,
    title: String,
    status: RecordStatus = RecordStatus.Active,
    supersededBy: String? = null,
) = LearningRecord(
    id,
    1,
    kind,
    title,
    "Pensavi che `a` restasse valida dopo `let b = a;`. Ora prevedi l'errore del compilatore.",
    "Hai sbagliato la domanda sul move e poi hai spiegato perché `a` non è più valida.",
    status,
    supersededBy,
    "l2",
    Instant.EPOCH,
)

private val previewActions = ProgressTabActions({}, {}, {}, {}, {}, {})

private val previewTerms =
    listOf(
        GlossaryTerm(
            "1",
            "borrowing",
            "borrowing",
            "L'atto di prendere in prestito un valore senza diventarne proprietari.",
            emptyList(),
            "l2",
            Instant.EPOCH,
            Instant.EPOCH,
        ),
        GlossaryTerm(
            "2",
            "ownership",
            "ownership",
            "La regola per cui ogni valore ha un solo proprietario.",
            listOf("proprietà"),
            "l2",
            Instant.EPOCH,
            Instant.EPOCH,
        ),
    )

@Composable
private fun TabSample(state: ProgressUiState) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        ProgressTab(state, previewActions, {
            LessonOrigin(it.orEmpty(), 2, "Ownership")
        }, hasUnfinishedLesson = true, allLessonsCompleted = false)
    }
}

private val registerState =
    ProgressUiState(
        records =
            ListState(
                listOf(
                    record(
                        "r2",
                        RecordKind.MisconceptionCorrected,
                        "Dopo un move la variabile di partenza non è più utilizzabile",
                    ),
                    record(
                        "r1",
                        RecordKind.Demonstrated,
                        "Sai prevedere quando un valore viene spostato",
                        RecordStatus.Superseded,
                        "r2",
                    ),
                ),
                loading = false,
            ),
    )

@Preview(name = "Register light", showBackground = true, widthDp = 420, heightDp = 1000)
@Composable
private fun RegisterLightPreview() = TeachTheme(darkTheme = false) { TabSample(registerState) }

@Preview(name = "Register dark", showBackground = true, widthDp = 420, heightDp = 1000)
@Composable
private fun RegisterDarkPreview() = TeachTheme(darkTheme = true) { TabSample(registerState) }

@Preview(name = "Register empty", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun RegisterEmptyPreview() =
    TeachTheme(darkTheme = false) { TabSample(ProgressUiState(records = ListState(emptyList(), loading = false))) }

@Preview(name = "Glossary light", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun GlossaryLightPreview() =
    TeachTheme(darkTheme = false) {
        TabSample(
            ProgressUiState(section = ProgressSection.Glossary, glossary = ListState(previewTerms, loading = false)),
        )
    }

@Preview(name = "Glossary dark", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun GlossaryDarkPreview() =
    TeachTheme(darkTheme = true) {
        TabSample(
            ProgressUiState(section = ProgressSection.Glossary, glossary = ListState(previewTerms, loading = false)),
        )
    }

@Preview(name = "Register font 2.0", showBackground = true, widthDp = 360, heightDp = 1600, fontScale = 2f)
@Composable
private fun RegisterLargePreview() = TeachTheme(darkTheme = false) { TabSample(registerState) }
