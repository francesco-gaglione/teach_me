package com.teachme.core.data

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Folder
import com.teachme.core.model.Page
import com.teachme.core.network.FolderApi
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class FolderRepositoryTest {
    private fun folder(n: Int) = Folder("id$n", "F$n", n, 0, Instant.EPOCH, Instant.EPOCH)

    private class PagedApi(
        private val pages: Map<String?, ApiResult<Page<Folder>>>,
    ) : FolderApi {
        val requested = mutableListOf<Pair<String?, Int?>>()

        override suspend fun listFolders(
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Folder>> {
            requested += cursor to limit
            return pages.getValue(cursor)
        }

        override suspend fun getFolder(id: String) = error("unused")

        override suspend fun createFolder(name: String) = error("unused")

        override suspend fun renameFolder(
            id: String,
            name: String,
        ) = error("unused")

        override suspend fun deleteFolder(id: String) = error("unused")
    }

    @Test
    fun `follows cursors across pages in order`() =
        runTest {
            val api =
                PagedApi(
                    mapOf(
                        null to ApiResult.Success(Page(listOf(folder(0), folder(1)), "c1")),
                        "c1" to ApiResult.Success(Page(listOf(folder(2)), "c2")),
                        "c2" to ApiResult.Success(Page(listOf(folder(3)), null)),
                    ),
                )

            val result = DefaultFolderRepository(api).listFolders()

            assertEquals(listOf("id0", "id1", "id2", "id3"), (result as ApiResult.Success).value.map { it.id })
            assertEquals(listOf<String?>(null, "c1", "c2"), api.requested.map { it.first })
            assertTrue(api.requested.all { it.second == 200 })
        }

    @Test
    fun `a failing page aborts the whole listing`() =
        runTest {
            val api =
                PagedApi(
                    mapOf(
                        null to ApiResult.Success(Page(listOf(folder(0)), "c1")),
                        "c1" to ApiResult.Failure(ApiFailure.Unauthorized),
                    ),
                )

            assertEquals(ApiResult.Failure(ApiFailure.Unauthorized), DefaultFolderRepository(api).listFolders())
        }

    @Test
    fun `a backend that never ends the cursor chain fails instead of returning a truncated list`() =
        runTest {
            val api =
                PagedApi(
                    mapOf(
                        null to ApiResult.Success(Page(listOf(folder(0)), "loop")),
                        "loop" to ApiResult.Success(Page(listOf(folder(1)), "loop")),
                    ),
                )

            val result = DefaultFolderRepository(api).listFolders()

            assertEquals(MAX_PAGES, api.requested.size)
            assertEquals(ApiResult.Failure(ApiFailure.Unexpected(httpStatus = null)), result)
        }
}
