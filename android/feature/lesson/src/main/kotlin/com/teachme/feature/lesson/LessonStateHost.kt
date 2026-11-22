package com.teachme.feature.lesson

import com.teachme.sdui.model.Exercise
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** What the collaborators of [LessonViewModel] share: the state, the scope, and the exercises of the open lesson. */
internal class LessonStateHost(
    val key: String,
    val scope: CoroutineScope,
    val dispatchers: LessonDispatchers,
    val state: MutableStateFlow<LessonUiState>,
    val persistRequests: Channel<Unit>,
) {
    val isSample = key.startsWith(LESSON_FIXTURE_PREFIX)
    var exercises: Map<String, Exercise> = emptyMap()
    var fromCache = false

    inline fun mutate(transform: (LessonUiState.Ready) -> LessonUiState.Ready) {
        state.update { current -> if (current is LessonUiState.Ready) transform(current) else current }
        persistRequests.trySend(Unit)
    }

    fun setAuthError(value: Boolean) =
        state.update { if (it is LessonUiState.Ready && it.authError != value) it.copy(authError = value) else it }

    fun fail(next: LessonUiState) {
        state.value = next
    }
}
