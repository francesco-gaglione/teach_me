package com.teachme.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListRow
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTopBar

/** `settings/licenses`: static list read from the bundled `licenses.json`, rendered with our tokens. */
@Composable
fun LicensesRoute(onBack: () -> Unit) {
    val context = LocalContext.current
    val entries =
        remember {
            val raw =
                runCatching {
                    context.assets
                        .open(
                            LICENSES_ASSET,
                        ).bufferedReader()
                        .use { it.readText() }
                }.getOrNull()
            parseLicenses(raw)
        }
    LicensesScreen(entries, onBack)
}

@Composable
internal fun LicensesScreen(
    entries: List<LicenseEntry>?,
    onBack: () -> Unit,
) {
    TeachScreen(
        topBar = {
            TeachTopBar(
                title = stringResource(R.string.licenses_title),
                navigation = {
                    TeachIconButton(
                        TeachIcons.ChevronLeft,
                        stringResource(R.string.settings_back),
                        onBack,
                        tint = TeachTheme.colors.ink,
                    )
                },
            )
        },
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            if (entries == null) {
                TeachText(
                    stringResource(R.string.licenses_unavailable),
                    Modifier.padding(TeachTheme.layout.screenMarginCompact),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
            } else {
                var expanded by rememberSaveable { mutableStateOf<String?>(null) }
                LazyColumn(
                    Modifier.widthIn(max = TeachTheme.layout.contentMaxWidth).fillMaxWidth(),
                    contentPadding =
                        androidx.compose.foundation.layout.PaddingValues(
                            TeachTheme.layout.screenMarginCompact,
                        ),
                ) {
                    item {
                        TeachCard(Modifier.fillMaxWidth()) {
                            Column {
                                entries.forEachIndexed { index, entry ->
                                    if (index > 0) TeachRowDivider()
                                    LicenseRow(entry, entry.name == expanded) {
                                        expanded = if (expanded == entry.name) null else entry.name
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LicenseRow(
    entry: LicenseEntry,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val subtitle = stringResource(R.string.licenses_row_subtitle, entry.licenseName, entry.version)
    val state = stringResource(if (expanded) R.string.licenses_state_open else R.string.licenses_state_closed)
    Column {
        TeachListRow(
            title = entry.name,
            onClick = onToggle,
            contentDescription = "${entry.name}, $subtitle, $state",
            subtitle = subtitle,
            trailing = {
                TeachIcon(
                    if (expanded) TeachIcons.ChevronUp else TeachIcons.ChevronDown,
                    contentDescription = null,
                    tint = TeachTheme.colors.inkSoft,
                )
            },
        )
        if (expanded) {
            Column(
                Modifier.padding(
                    start = TeachTheme.spacing.md,
                    end = TeachTheme.spacing.md,
                    bottom = TeachTheme.spacing.md,
                ),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                if (entry.copyright.isNotBlank()) {
                    TeachText(
                        stringResource(R.string.licenses_copyright, entry.copyright),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
                if (entry.url.isNotBlank()) {
                    TeachText(
                        entry.url,
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
                if (entry.text.isNotBlank()) {
                    TeachText(
                        entry.text,
                        Modifier.semantics { contentDescription = entry.text },
                        style = TeachTheme.typography.code.inline,
                        color = TeachTheme.colors.ink,
                    )
                }
            }
        }
    }
}

private val previewEntries =
    listOf(
        LicenseEntry(
            "Ktor",
            "3.6.1",
            "Apache License 2.0",
            "JetBrains s.r.o.",
            "Apache License\nVersion 2.0, January 2004",
        ),
        LicenseEntry(
            "Inter (font)",
            "variable",
            "SIL Open Font License 1.1",
            "The Inter Project Authors",
            "SIL OPEN FONT LICENSE",
        ),
    )

@Preview(name = "Licenses", showBackground = true)
@Composable
private fun LicensesLightPreview() = TeachTheme(darkTheme = false) { LicensesScreen(previewEntries, {}) }

@Preview(name = "Licenses dark", showBackground = true)
@Composable
private fun LicensesDarkPreview() = TeachTheme(darkTheme = true) { LicensesScreen(previewEntries, {}) }

@Preview(name = "Licenses missing", showBackground = true)
@Composable
private fun LicensesMissingPreview() = TeachTheme(darkTheme = false) { LicensesScreen(null, {}) }
