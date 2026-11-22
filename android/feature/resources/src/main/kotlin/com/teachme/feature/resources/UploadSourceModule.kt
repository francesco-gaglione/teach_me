package com.teachme.feature.resources

import android.content.Context
import android.content.Intent
import android.net.Uri
import com.teachme.core.model.UploadSource
import com.teachme.core.model.UploadSourceRestorer
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent

@Module
@InstallIn(SingletonComponent::class)
internal object UploadSourceModule {
    @Provides
    fun provideUploadSourceRestorer(
        @ApplicationContext context: Context,
    ): UploadSourceRestorer =
        object : UploadSourceRestorer {
            override fun restore(
                reference: String,
                displayName: String,
            ): UploadSource? = ContentUriSource.restore(context.contentResolver, reference, displayName)

            @Suppress("UseKtx") // core-ktx is not a dependency of this module
            override fun release(reference: String) {
                try {
                    context.contentResolver.releasePersistableUriPermission(
                        Uri.parse(reference),
                        Intent.FLAG_GRANT_READ_URI_PERMISSION,
                    )
                } catch (_: SecurityException) {
                    // No persisted grant for this URI: nothing to give back.
                }
            }
        }
}
