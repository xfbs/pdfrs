package net.xfbs.specto

import android.util.Log
import androidx.compose.runtime.mutableStateListOf
import java.io.File
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject
import uniffi.specto_ffi.SavedView

/** Documents remembered beyond this many are forgotten, oldest first. */
private const val MAX_DOCUMENTS = 200

/** How many documents the start screen lists. */
private const val MAX_RECENTS = 20

/** Bytes hashed at each end of a file to recognize it. */
private const val FINGERPRINT_SPAN = 64 * 1024

/**
 * What the app remembers about documents between sessions: where the reader was in each, and which
 * ones can be reopened from the start screen.
 *
 * A document is recognized by its content ([fingerprint]), not by where it came from: the same file
 * arrives under different addresses depending on the app that hands it over. Kept in a small JSON
 * [file] (in the app's private storage). Main thread only.
 */
class DocumentStore(private val file: File) {
    class Entry(
        val key: String,
        val name: String,
        /** Where to reopen it from, if the app may still read that address. */
        val uri: String?,
        /** When it was last opened, in milliseconds since the epoch. */
        val opened: Long,
        val pageCount: Int,
        val view: SavedView?,
        /** On wide windows: whether the sidebar was open. */
        val sidebar: Boolean = true,
    )

    /** Most recently opened first. Observable, so the start screen follows. */
    private val entries = mutableStateListOf<Entry>()

    init {
        try {
            if (file.exists()) {
                val array = JSONObject(file.readText()).getJSONArray("documents")
                for (i in 0 until array.length()) entries += array.getJSONObject(i).toEntry()
            }
        } catch (e: Exception) {
            // Unreadable (written by a newer version, say): start afresh.
            Log.w("specto", "ignoring $file", e)
            entries.clear()
        }
        forgetUnoffered()
    }

    /** Documents that can be reopened, most recent first. */
    val recents: List<Entry> get() = entries.filter { it.uri != null }.take(MAX_RECENTS)

    fun find(key: String): Entry? = entries.firstOrNull { it.key == key }

    /**
     * Notes that [key] was just opened.
     *
     * [uri] is where to reopen it from, or null if that won't be possible. An address known from
     * before is kept. An address leads to one document: any other one that was found at [uri]
     * before is no longer offered from there.
     */
    fun opened(key: String, name: String, uri: String?, pageCount: Int) {
        val known = find(key)
        entries.removeAll { it.key == key }
        // What is at this address now is this document. Another one found there before (the file
        // was saved over, or replaced) isn't any more.
        if (uri != null) {
            for (i in entries.indices) {
                if (entries[i].uri == uri) entries[i] = entries[i].withoutUri()
            }
        }
        entries.add(
            0,
            Entry(
                key,
                name,
                uri ?: known?.uri,
                System.currentTimeMillis(),
                pageCount,
                known?.view,
                known?.sidebar ?: true,
            ),
        )
        while (entries.size > MAX_DOCUMENTS) entries.removeAt(entries.lastIndex)
        forgetUnoffered()
        save()
    }

    /** Remembers where the reader is in [key], and whether the sidebar is open. */
    fun saveView(key: String, view: SavedView, sidebar: Boolean = true) {
        val i = entries.indexOfFirst { it.key == key }
        if (i < 0 || (entries[i].view == view && entries[i].sidebar == sidebar)) return
        val e = entries[i]
        entries[i] = Entry(e.key, e.name, e.uri, e.opened, e.pageCount, view, sidebar)
        save()
    }

    /** [uri] can't be opened any more: stop offering it. The reading position stays. */
    fun unreachable(uri: String) {
        var changed = false
        for (i in entries.indices) {
            if (entries[i].uri == uri) {
                entries[i] = entries[i].withoutUri()
                changed = true
            }
        }
        if (changed) save()
    }

    /** Addresses the start screen may still offer. The rest need no read permission any more. */
    fun reopenable(): Set<String> = recents.mapNotNull { it.uri }.toSet()

    /**
     * Forgets the addresses of documents past those the start screen lists. Their read permission
     * is given back, so they couldn't be opened from there if one listed before them went.
     */
    private fun forgetUnoffered() {
        var offered = 0
        for (i in entries.indices) {
            if (entries[i].uri != null && ++offered > MAX_RECENTS) {
                entries[i] = entries[i].withoutUri()
            }
        }
    }

    private fun Entry.withoutUri() = Entry(key, name, null, opened, pageCount, view, sidebar)

    private fun save() {
        val array = JSONArray()
        for (e in entries) array.put(e.toJson())
        try {
            // Via a temporary file, so a crash can't leave half a file behind.
            val temporary = File(file.path + ".new")
            temporary.writeText(JSONObject().put("documents", array).toString())
            if (!temporary.renameTo(file)) Log.w("specto", "could not replace $file")
        } catch (e: Exception) {
            Log.w("specto", "could not write $file", e)
        }
    }

    companion object {
        /**
         * Identifies a file of [size] bytes by its length and both ends: quick even for huge files,
         * which needn't be read for it, and stable however the file reaches the app.
         *
         * [read] returns the given number of bytes from the given offset.
         */
        fun fingerprint(size: Long, read: (offset: Long, length: Int) -> ByteArray): String {
            val digest = MessageDigest.getInstance("SHA-256")
            digest.update(size.toString().toByteArray())
            digest.update(read(0, minOf(FINGERPRINT_SPAN.toLong(), size).toInt()))
            if (size > FINGERPRINT_SPAN) {
                digest.update(
                    read(size - FINGERPRINT_SPAN, FINGERPRINT_SPAN),
                )
            }
            return digest.digest().take(16).joinToString("") { "%02x".format(it) }
        }

        /** The same for a file held in memory. */
        fun fingerprint(pdf: ByteArray): String = fingerprint(pdf.size.toLong()) { offset, length ->
            pdf.copyOfRange(offset.toInt(), offset.toInt() + length)
        }
    }
}

private fun DocumentStore.Entry.toJson(): JSONObject {
    val json = JSONObject()
        .put("key", key)
        .put("name", name)
        .put("opened", opened)
        .put("pages", pageCount)
    if (uri != null) json.put("uri", uri)
    if (!sidebar) json.put("sidebar", false)
    view?.let {
        json.put("page", it.page.toInt()).put("offset", it.pageOffset).put("paged", it.paged)
        it.zoom?.let { zoom -> json.put("zoom", zoom) }
        if (it.twoPages) json.put("two", true)
        if (it.coverAlone) json.put("cover", true)
        // Only when it isn't the mode's own fit.
        if (it.fitWidth == it.paged) json.put("fitWidth", it.fitWidth)
    }
    return json
}

private fun JSONObject.toEntry() = DocumentStore.Entry(
    key = getString("key"),
    name = getString("name"),
    uri = if (has("uri")) getString("uri") else null,
    opened = getLong("opened"),
    pageCount = optInt("pages", 0),
    sidebar = optBoolean("sidebar", true),
    view = if (has("page")) {
        SavedView(
            page = getInt("page").coerceAtLeast(0).toUInt(),
            pageOffset = optDouble("offset", 0.0),
            paged = optBoolean("paged", true),
            zoom = if (has("zoom")) getDouble("zoom") else null,
            twoPages = optBoolean("two", false),
            coverAlone = optBoolean("cover", false),
            fitWidth = optBoolean("fitWidth", !optBoolean("paged", true)),
        )
    } else {
        null
    },
)
