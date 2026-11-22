package com.teachme.feature.resources

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.JobStatus
import com.teachme.core.model.MAX_DOCUMENTS_PER_TOPIC

// Previews of "I tuoi documenti": empty, reading, ready, failed (every code), limit, upload errors, banner, dark theme.

@Composable
private fun PreviewSection(
    state: DocumentsUiState,
    dark: Boolean = false,
    missionPending: Boolean = false,
    banner: Boolean = false,
) = TeachTheme(darkTheme = dark) {
    Box(Modifier.background(TeachTheme.colors.bg).padding(16.dp)) {
        DocumentsSection(
            state,
            missionPending,
            bannerDocuments = if (banner) state.documents else emptyList(),
            acting = false,
            onRefreshSources = {},
            actions = PreviewData.noDocumentActions,
        )
    }
}

private fun ready(
    vararg documents: com.teachme.core.model.TopicDocument,
    job: com.teachme.core.model.TopicJob? = null,
    upload: UploadUi = UploadUi.Idle,
) = DocumentsUiState(load = DocumentsLoad.Ready, documents = documents.toList(), importJob = job, upload = upload)

@Preview(name = "Docs empty", showBackground = true, widthDp = 640, heightDp = 560)
@Composable
private fun EmptyPreview() = PreviewSection(ready())

@Preview(name = "Docs empty dark", showBackground = true, widthDp = 640, heightDp = 560)
@Composable
private fun EmptyDarkPreview() = PreviewSection(ready(), dark = true)

@Preview(name = "Docs ready", showBackground = true, widthDp = 640, heightDp = 420)
@Composable
private fun ReadyPreview() = PreviewSection(ready(PreviewData.document()), missionPending = true)

@Preview(name = "Docs ready dark", showBackground = true, widthDp = 640, heightDp = 420)
@Composable
private fun ReadyDarkPreview() = PreviewSection(ready(PreviewData.document()), dark = true)

@Preview(name = "Docs ready font 2.0 compact", showBackground = true, widthDp = 360, heightDp = 800, fontScale = 2f)
@Composable
private fun ReadyLargePreview() = PreviewSection(ready(PreviewData.document()))

@Preview(name = "Docs reading", showBackground = true, widthDp = 640, heightDp = 420)
@Composable
private fun ReadingPreview() =
    PreviewSection(
        ready(
            PreviewData.document(status = DocumentStatus.Importing, title = "rust-in-action"),
            job = PreviewData.importJob(JobStatus.Running),
        ),
    )

@Preview(name = "Docs uploading", showBackground = true, widthDp = 640, heightDp = 360)
@Composable
private fun UploadingPreview() = PreviewSection(ready(upload = UploadUi.Sending("rust-in-action.pdf")))

@Preview(name = "Docs failed scan", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun FailedScanPreview() =
    PreviewSection(ready(PreviewData.document(status = DocumentStatus.Failed, errorCode = "no_text")))

@Preview(name = "Docs failed scan dark", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun FailedScanDarkPreview() =
    PreviewSection(ready(PreviewData.document(status = DocumentStatus.Failed, errorCode = "no_text")), dark = true)

@Preview(name = "Docs failed password", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun FailedPasswordPreview() =
    PreviewSection(ready(PreviewData.document(status = DocumentStatus.Failed, errorCode = "encrypted_pdf")))

@Preview(name = "Docs failed retryable", showBackground = true, widthDp = 640, heightDp = 560)
@Composable
private fun FailedRetryablePreview() {
    val failed =
        PreviewData.importJob(JobStatus.Failed).copy(
            error =
                com.teachme.core.model
                    .JobError("timeout", "x"),
        )
    PreviewSection(ready(PreviewData.document(status = DocumentStatus.Failed, errorCode = "timeout"), job = failed))
}

@Preview(name = "Docs limit reached", showBackground = true, widthDp = 640, heightDp = 900)
@Composable
private fun LimitPreview() =
    PreviewSection(
        ready(*Array(MAX_DOCUMENTS_PER_TOPIC) { PreviewData.document(id = "d$it", title = "Documento ${it + 1}") }),
    )

@Preview(name = "Docs upload too large", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun TooLargePreview() = PreviewSection(ready(upload = UploadUi.Failed(UploadError.TooLarge)))

@Preview(name = "Docs upload quota exceeded", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun QuotaPreview() =
    PreviewSection(ready(PreviewData.document(), upload = UploadUi.Failed(UploadError.QuotaExceeded)))

@Preview(name = "Docs upload quota exceeded dark", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun QuotaDarkPreview() =
    PreviewSection(ready(PreviewData.document(), upload = UploadUi.Failed(UploadError.QuotaExceeded)), dark = true)

@Preview(name = "Docs upload network error", showBackground = true, widthDp = 640, heightDp = 520)
@Composable
private fun NetworkErrorPreview() =
    PreviewSection(ready(PreviewData.document(), upload = UploadUi.Failed(UploadError.Network)))

@Preview(name = "Docs banner", showBackground = true, widthDp = 640, heightDp = 560)
@Composable
private fun BannerPreview() = PreviewSection(ready(PreviewData.document()), banner = true)

@Preview(name = "Docs banner dark", showBackground = true, widthDp = 640, heightDp = 560)
@Composable
private fun BannerDarkPreview() = PreviewSection(ready(PreviewData.document()), dark = true, banner = true)
