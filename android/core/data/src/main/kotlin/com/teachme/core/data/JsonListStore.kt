package com.teachme.core.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.UserScope
import com.teachme.core.model.UserScopes
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.jsonArray

/**
 * A list kept as one JSON string in a DataStore: it survives a restart, and every change is one serialized `edit`, so
 * concurrent writers never lose an item. Shared by the persistent queues (attempts, review grades).
 *
 * One list per user: the preference is named `<keyName>:<user id>@<backend address>` ([UserScope.key]), so what a user
 * left in the queue is only ever read, sent or counted under that user (ADR 0008). Without a scope (nobody ever signed
 * in) the list is empty and writes are dropped: there is no screen that could produce one.
 */
internal class JsonListStore<T>(
    private val store: DataStore<Preferences>,
    private val keyName: String,
    itemSerializer: KSerializer<T>,
    private val scopes: UserScopes,
) {
    private val serializer = ListSerializer(itemSerializer)

    private fun keyOf(scope: UserScope) = stringPreferencesKey(scopedKeyName(keyName, scope))

    private fun Preferences.decode(scope: UserScope): List<T> {
        val text = this[keyOf(scope)] ?: return emptyList()
        // Damaged data is dropped, never a crash: what it held can be redone from the screens.
        return runCatching { JSON.decodeFromString(serializer, text) }.getOrDefault(emptyList())
    }

    suspend fun read(): List<T> {
        val scope = scopes.current() ?: return emptyList()
        return store.data.first().decode(scope)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    fun observe(): Flow<List<T>> =
        scopes.scope.flatMapLatest { scope ->
            if (scope == null) flowOf(emptyList()) else store.data.map { it.decode(scope) }
        }

    suspend fun change(transform: (List<T>) -> List<T>) {
        val scope = scopes.current() ?: return
        store.edit { prefs ->
            val next = transform(prefs.decode(scope))
            val key = keyOf(scope)
            if (next.isEmpty()) prefs.remove(key) else prefs[key] = JSON.encodeToString(serializer, next)
        }
    }

    private companion object {
        val JSON = Json { ignoreUnknownKeys = true }
    }
}

internal fun scopedKeyName(
    keyName: String,
    scope: UserScope,
) = "$keyName:${scope.key}"

/**
 * Moves the list the single-user versions kept under [keyName] to [scope] (merging with what is already there). Done
 * on raw JSON so no item type is needed. The unscoped key is removed, so the next user to sign in finds nothing: the
 * first one claims it.
 */
internal suspend fun claimLegacyList(
    store: DataStore<Preferences>,
    keyName: String,
    scope: UserScope,
) {
    val legacyKey = stringPreferencesKey(keyName)
    val scopedKey = stringPreferencesKey(scopedKeyName(keyName, scope))
    store.edit { prefs ->
        val legacy = prefs[legacyKey] ?: return@edit
        prefs.remove(legacyKey)
        val own = prefs[scopedKey]
        prefs[scopedKey] =
            if (own == null) {
                legacy
            } else {
                runCatching {
                    JsonArray(
                        Json.parseToJsonElement(legacy).jsonArray + Json.parseToJsonElement(own).jsonArray,
                    ).toString()
                }.getOrDefault(own)
            }
    }
}

/**
 * Moves the preferences the single-user versions kept under plain names (`isLegacy`) to [scope]: each becomes
 * `<scope>|<name>`. Strings and string sets only, which is all those stores hold. Merges nothing: the first user to sign in
 * claims them and the plain names are gone for the next.
 */
internal suspend fun claimLegacyKeys(
    store: DataStore<Preferences>,
    scope: UserScope,
    isLegacy: (String) -> Boolean,
) {
    store.edit { prefs ->
        for ((key, value) in prefs.asMap().entries.toList()) {
            if (!isLegacy(key.name)) continue
            val target = "${scope.key}$SCOPE_SEPARATOR${key.name}"
            @Suppress("UNCHECKED_CAST")
            when (value) {
                is String -> prefs[stringPreferencesKey(target)] = value
                is Set<*> ->
                    prefs[
                        androidx.datastore.preferences.core
                            .stringSetPreferencesKey(target),
                    ] =
                        value as Set<String>
            }
            prefs.remove(key)
        }
    }
}
