package com.teachme.feature.resources

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import com.teachme.core.model.UploadSource
import java.io.FileNotFoundException
import java.io.InputStream

/**
 * A file chosen with the system picker, read through its `content://` URI. Nothing is copied: [open] gives the stream
 * of the provider, which the HTTP client drains block by block while it sends the request.
 */
internal class ContentUriSource private constructor(
    private val resolver: ContentResolver,
    private val uri: Uri,
    override val displayName: String,
    override val sizeBytes: Long?,
) : UploadSource {
    override fun open(): InputStream = resolver.openInputStream(uri) ?: throw FileNotFoundException(uri.toString())

    companion object {
        /** Reads name and size from the provider; blocking, so call it off the main thread. */
        fun resolve(
            resolver: ContentResolver,
            uri: Uri,
        ): ContentUriSource {
            var name: String? = null
            var size: Long? = null
            runCatching {
                resolver
                    .query(
                        uri,
                        arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE),
                        null,
                        null,
                        null,
                    )?.use {
                        if (it.moveToFirst()) {
                            val nameColumn = it.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                            val sizeColumn = it.getColumnIndex(OpenableColumns.SIZE)
                            if (nameColumn >= 0 && !it.isNull(nameColumn)) name = it.getString(nameColumn)
                            if (sizeColumn >= 0 &&
                                !it.isNull(sizeColumn)
                            ) {
                                size = it.getLong(sizeColumn).takeIf { s -> s >= 0 }
                            }
                        }
                    }
            }
            if (size == null) {
                size =
                    runCatching { resolver.openAssetFileDescriptor(uri, "r")?.use { it.length } }
                        .getOrNull()
                        ?.takeIf { it >= 0 }
            }
            val fallback = uri.lastPathSegment?.substringAfterLast('/').orEmpty()
            return ContentUriSource(
                resolver,
                uri,
                name?.takeIf { it.isNotBlank() } ?: fallback.ifBlank { "document.pdf" },
                size,
            )
        }
    }
}
