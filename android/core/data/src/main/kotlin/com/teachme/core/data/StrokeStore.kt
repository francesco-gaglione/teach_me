package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeStore
import com.teachme.core.model.UserScope
import com.teachme.core.model.UserScopes
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.io.File
import java.net.URLEncoder
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/**
 * The handwritten notes of every lesson not yet sent (and the last ones the backend gave, for opening offline), plus the
 * pen choices of the device, all per user (ADR 0008).
 *
 * A set can weigh megabytes and the pen writes often, so each lesson has its own file,
 * `<root>/strokes/<user>/<lesson>.json`, written whole and atomically (temporary file, then rename); changes to one
 * lesson are serialized. The pen choices stay in [store]. A damaged file reads as "no local notes for the lesson".
 *
 * The first line of a file is a small header (lesson, `dirty`, `parked`), the second the whole set. Counting the unsent
 * lessons and pruning read the first line only, never the points. The header and the set are in one file, so they
 * can never disagree.
 */
class DataStoreStrokeStore(
    private val store: DataStore<Preferences>,
    private val scopes: UserScopes,
    private val root: File,
) : StrokeStore {
    private val locks = ConcurrentHashMap<String, Mutex>()
    private val lastStamp = AtomicLong()

    private fun dirOf(scope: UserScope) = File(File(root, STROKES_KEY), safeName(scope.key))

    private fun fileOf(
        scope: UserScope,
        lessonId: String,
    ) = File(dirOf(scope), safeName(lessonId) + EXT)

    override suspend fun get(lessonId: String): StoredStrokes? {
        val scope = scopes.current() ?: return null
        val file = fileOf(scope, lessonId)
        return withContext(Dispatchers.IO) {
            read(file) ?: run {
                // A file whose points cannot be read is no notes at all: remove it, so that its header (which still
                // says "unsent") does not count a lesson that can never be sent.
                if (file.isFile) {
                    lockOf(file).withLock { if (read(file) == null) file.delete() }
                }
                null
            }
        }
    }

    override suspend fun update(
        lessonId: String,
        transform: (StoredStrokes?) -> StoredStrokes?,
    ) {
        val scope = scopes.current() ?: return
        val file = fileOf(scope, lessonId)
        withContext(Dispatchers.IO) {
            lockOf(file).withLock {
                val old = read(file)
                val new = transform(old)?.copy(lessonId = lessonId)
                when {
                    new == null -> file.delete()
                    // A copy of the backend's empty set says nothing more than having no entry.
                    !new.dirty && new.items.isEmpty() -> file.delete()
                    // Nothing changed (the server's set is the one already kept): no write, no new place in the order.
                    old == new -> Unit
                    else -> {
                        write(file, new)
                        if (!new.dirty) prune(dirOf(scope), keep = file)
                    }
                }
            }
        }
    }

    override suspend fun dirtyLessons(): List<String> {
        val scope = scopes.current() ?: return emptyList()
        return withContext(Dispatchers.IO) {
            files(dirOf(scope)).mapNotNull { readHeader(it) }.filter { it.dirty }.map { it.lessonId }
        }
    }

    private fun lockOf(file: File) = locks.getOrPut(file.path) { Mutex() }

    /** Oldest change first. */
    private fun files(dir: File): List<File> =
        dir
            .listFiles { f -> f.isFile && f.name.endsWith(EXT) }
            .orEmpty()
            .sortedBy { it.lastModified() }

    private fun read(file: File): StoredStrokes? {
        if (!file.isFile) return null
        // Damaged data is dropped, never a crash: the lesson is read from the backend again.
        return runCatching {
            val text = file.readText()
            JSON.decodeFromString(StoredStrokes.serializer(), text.substring(text.indexOf('\n') + 1))
        }.getOrNull()
    }

    /** Only the first line: what the counts and the pruning need, without decoding a single point. */
    private fun readHeader(file: File): Header? {
        if (!file.isFile) return null
        return runCatching {
            val line = file.bufferedReader().use { it.readLine() }
            val o = JSON.parseToJsonElement(line).jsonObject
            Header(
                o.getValue("lessonId").jsonPrimitive.content,
                o.getValue("dirty").jsonPrimitive.boolean,
                o["parked"]?.jsonPrimitive?.boolean ?: false,
            )
        }.getOrNull()
    }

    private fun write(
        file: File,
        value: StoredStrokes,
    ) {
        file.parentFile?.mkdirs()
        val tmp = File(file.parentFile, file.name + TMP)
        val header = Header(value.lessonId, value.dirty, value.parked)
        tmp.writeText(
            header.toJson() + "\n" +
                JSON.encodeToString(StoredStrokes.serializer(), value),
        )
        Files.move(tmp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        // Strictly increasing, so "most recent" never ties.
        file.setLastModified(lastStamp.updateAndGet { maxOf(System.currentTimeMillis(), it + 1) })
    }

    /**
     * Sets with unsent changes are never dropped; of the others (copies of the backend's set kept to open a lesson
     * offline) only the [MAX_CLEAN_STROKE_SETS] most recent stay. [keep] is the file just written.
     */
    private suspend fun prune(
        dir: File,
        keep: File,
    ) {
        val all = files(dir)
        if (all.size <= MAX_CLEAN_STROKE_SETS) return
        var clean = all.count { readHeader(it)?.dirty == false }
        for (f in all) {
            if (clean <= MAX_CLEAN_STROKE_SETS) break
            if (f == keep) continue
            val lock = lockOf(f)
            if (lock.tryLock()) {
                try {
                    if (readHeader(f)?.dirty != true) {
                        f.delete()
                        clean--
                    }
                } finally {
                    lock.unlock()
                }
            }
        }
    }

    override suspend fun penTool() = readPref(PEN_TOOL_KEY)

    override suspend fun setPenTool(tool: String) = writePref(PEN_TOOL_KEY, tool)

    override suspend fun inkColor() = readPref(PEN_INK_COLOR_KEY)

    override suspend fun setInkColor(color: String) = writePref(PEN_INK_COLOR_KEY, color)

    override suspend fun penWidth() = readPref(PEN_WIDTH_KEY)

    override suspend fun setPenWidth(width: String) = writePref(PEN_WIDTH_KEY, width)

    private suspend fun readPref(name: String): String? {
        val scope = scopes.current() ?: return null
        return store.data.first()[stringPreferencesKey(scopedKeyName(name, scope))]
    }

    private suspend fun writePref(
        name: String,
        value: String,
    ) {
        val scope = scopes.current() ?: return
        store.edit { it[stringPreferencesKey(scopedKeyName(name, scope))] = value }
    }
}

private data class Header(
    val lessonId: String,
    val dirty: Boolean,
    val parked: Boolean,
) {
    fun toJson(): String =
        buildJsonObject {
            put("lessonId", lessonId)
            put("dirty", dirty)
            put("parked", parked)
        }.toString()
}

/** How many lessons keep a copy of their notes that has already been sent (the rest is read from the backend). */
internal const val MAX_CLEAN_STROKE_SETS = 30

internal const val STROKES_KEY = "strokes"
private const val EXT = ".json"
private const val TMP = ".tmp"
private val JSON = Json { ignoreUnknownKeys = true }

/** A name safe for a file: nothing that could leave the folder or clash on another file system. */
internal fun safeName(name: String): String = URLEncoder.encode(name, "UTF-8").replace("*", "%2A").replace(".", "%2E")

internal const val PEN_TOOL_KEY = "pen_tool"
internal const val PEN_INK_COLOR_KEY = "pen_ink_color"
internal const val PEN_WIDTH_KEY = "pen_width"

internal val Context.strokeDataStore: DataStore<Preferences> by preferencesDataStore(name = "strokes")
