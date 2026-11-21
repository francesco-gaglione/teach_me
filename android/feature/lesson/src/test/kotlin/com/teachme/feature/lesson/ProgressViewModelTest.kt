package com.teachme.feature.lesson

import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.RecordStatus
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProgressViewModelTest {
    private val backend = FakeLessons()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `refresh reads the register and the glossary and splits active from superseded records`() =
        runTest {
            backend.records =
                ApiResult.Success(
                    listOf(
                        record("r2", 2),
                        record("r1", 1, RecordStatus.Superseded, supersededBy = "r2"),
                    ),
                )
            backend.glossary = ApiResult.Success(listOf(term("ownership"), term("borrowing")))
            val vm = ProgressViewModel("T1", backend)
            vm.refresh()
            runCurrent()
            val s = vm.state.value
            assertEquals(listOf("r2"), s.activeRecords.map { it.id })
            assertEquals(listOf("r1"), s.supersededRecords.map { it.id })
            assertEquals(2, s.glossary.items?.size)
            assertFalse(s.records.loading)
            assertEquals(ProgressSection.Register, s.section)
            vm.selectSection(ProgressSection.Glossary)
            assertEquals(ProgressSection.Glossary, vm.state.value.section)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a first failure shows the error with Riprova, a later failure keeps the last list as stale`() =
        runTest {
            backend.records = ApiResult.Failure(ApiFailure.Unexpected(500))
            val vm = ProgressViewModel("T1", backend)
            vm.refresh()
            runCurrent()
            assertEquals(LoadError.Server, vm.state.value.records.error)
            assertNull(vm.state.value.records.items)
            backend.records = ApiResult.Success(listOf(record()))
            vm.retryRecords()
            assertTrue(vm.state.value.records.loading)
            runCurrent()
            assertEquals(
                1,
                vm.state.value.records.items
                    ?.size,
            )
            backend.records = ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout))
            vm.refresh()
            runCurrent()
            assertTrue(vm.state.value.records.stale)
            assertEquals(
                1,
                vm.state.value.records.items
                    ?.size,
            )
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a closing job that succeeds refreshes both lists and signals the update`() =
        runTest {
            val vm = ProgressViewModel("T1", backend)
            vm.refresh()
            runCurrent()
            assertEquals(0, vm.state.value.updatedSignal)
            backend.records = ApiResult.Success(listOf(record()))
            backend.glossary = ApiResult.Success(listOf(term("ownership")))
            vm.onEvent(ChatEvent.JobUpdated(job("lesson_completion", JobStatus.Running, lessonId = "L1")))
            runCurrent()
            assertEquals(0, vm.state.value.updatedSignal)
            vm.onEvent(ChatEvent.JobUpdated(job("lesson_completion", JobStatus.Succeeded, lessonId = "L1")))
            runCurrent()
            assertEquals(1, vm.state.value.updatedSignal)
            assertEquals(
                1,
                vm.state.value.records.items
                    ?.size,
            )
            assertEquals(
                1,
                vm.state.value.glossary.items
                    ?.size,
            )
            // A generation job is not a reason to reread them.
            vm.onEvent(ChatEvent.JobUpdated(job("lesson_generation", JobStatus.Succeeded)))
            assertEquals(1, vm.state.value.updatedSignal)
            vm.viewModelScope.cancel()
        }
}
