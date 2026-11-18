package net.xfbs.specto

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.specto_ffi.SavedView

class DocumentStoreTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val file: File by lazy { File(folder.root, "documents.json") }
    private val view = SavedView(
        page = 3u,
        pageOffset = 0.25,
        paged = false,
        zoom = 1.5,
        twoPages = true,
        coverAlone = true,
        fitWidth = false,
    )

    @Test
    fun remembersAcrossRestarts() {
        DocumentStore(file).apply {
            opened("k1", "A book.pdf", "content://docs/1", pageCount = 240)
            saveView("k1", view)
        }
        val entry = checkNotNull(DocumentStore(file).find("k1"))
        assertEquals("A book.pdf", entry.name)
        assertEquals("content://docs/1", entry.uri)
        assertEquals(240, entry.pageCount)
        assertEquals(view, entry.view)
    }

    @Test
    fun remembersAClosedSidebar() {
        DocumentStore(file).apply {
            opened("k1", "a.pdf", null, 3)
            assertEquals(true, find("k1")?.sidebar)
            saveView("k1", view, sidebar = false)
        }
        val store = DocumentStore(file)
        assertEquals(false, store.find("k1")?.sidebar)
        // Opening it again doesn't forget.
        store.opened("k1", "a.pdf", null, 3)
        assertEquals(false, store.find("k1")?.sidebar)
        store.saveView("k1", view, sidebar = true)
        assertEquals(true, DocumentStore(file).find("k1")?.sidebar)
    }

    @Test
    fun fittedViewHasNoZoom() {
        val fitted = view.copy(zoom = null, paged = true, twoPages = false, coverAlone = false)
        DocumentStore(file).apply {
            opened("k1", "a.pdf", null, 3)
            saveView("k1", fitted)
        }
        assertEquals(fitted, DocumentStore(file).find("k1")?.view)
    }

    @Test
    fun anAddressLeadsToOneDocument() {
        val store = DocumentStore(file)
        store.opened("before", "report.pdf", "content://docs/1", 12)
        store.saveView("before", view)
        // The file there was saved over: other content, so another document.
        store.opened("after", "report.pdf", "content://docs/1", 11)
        assertEquals(listOf("after"), store.recents.map { it.key })
        // What was there is still known, should it turn up again elsewhere.
        assertEquals(null, store.find("before")?.uri)
        assertEquals(view, store.find("before")?.view)
        store.opened("before", "copy of report.pdf", "content://docs/2", 12)
        assertEquals(listOf("before", "after"), store.recents.map { it.key })
    }

    @Test
    fun recentsAreTheReopenableOnesNewestFirst() {
        val store = DocumentStore(file)
        store.opened("k1", "one.pdf", "content://docs/1", 1)
        store.opened("k2", "from another app.pdf", null, 1)
        store.opened("k3", "three.pdf", "content://docs/3", 1)
        assertEquals(listOf("k3", "k1"), store.recents.map { it.key })
        assertEquals(setOf("content://docs/1", "content://docs/3"), store.reopenable())

        // Opening one again moves it to the front.
        store.opened("k1", "one.pdf", "content://docs/1", 1)
        assertEquals(listOf("k1", "k3"), store.recents.map { it.key })
    }

    @Test
    fun reopeningKeepsThePositionAndAKnownAddress() {
        val store = DocumentStore(file)
        store.opened("k1", "one.pdf", "content://docs/1", 10)
        store.saveView("k1", view)
        // The same file, this time handed over by an app whose address can't be kept.
        store.opened("k1", "one (copy).pdf", null, 10)
        val entry = checkNotNull(store.find("k1"))
        assertEquals("one (copy).pdf", entry.name)
        assertEquals("content://docs/1", entry.uri)
        assertEquals(view, entry.view)
    }

    @Test
    fun anUnreachableFileLeavesTheListButKeepsItsPosition() {
        val store = DocumentStore(file)
        store.opened("k1", "one.pdf", "content://docs/1", 10)
        store.saveView("k1", view)
        store.unreachable("content://docs/1")
        assertTrue(store.recents.isEmpty())
        assertEquals(view, DocumentStore(file).find("k1")?.view)
    }

    @Test
    fun savingAViewForAnUnknownDocumentDoesNothing() {
        val store = DocumentStore(file)
        store.saveView("nope", view)
        assertNull(store.find("nope"))
    }

    @Test
    fun oldDocumentsAreForgotten() {
        val store = DocumentStore(file)
        for (i in 1..205) store.opened("k$i", "$i.pdf", "content://docs/$i", 1)
        assertNull(store.find("k1"))
        assertEquals("205.pdf", store.find("k205")?.name)
        // The start screen shows fewer than are remembered.
        assertEquals(20, store.recents.size)
        assertEquals("k205", store.recents.first().key)
    }

    @Test
    fun documentsPastTheListedOnesCantBeReopened() {
        val store = DocumentStore(file)
        for (i in 1..21) store.opened("k$i", "$i.pdf", "content://docs/$i", 1)
        // The first one's read permission is given back: it isn't listed.
        assertNull(store.find("k1")?.uri)
        assertTrue("content://docs/1" !in store.reopenable())
        // Nor once one listed before it goes.
        store.unreachable("content://docs/21")
        assertEquals(19, store.recents.size)
        assertEquals("k2", store.recents.last().key)
    }

    @Test
    fun anUnreadableFileStartsAfresh() {
        file.writeText("{ not json")
        val store = DocumentStore(file)
        assertTrue(store.recents.isEmpty())
        store.opened("k1", "one.pdf", "content://docs/1", 1)
        assertEquals("one.pdf", DocumentStore(file).find("k1")?.name)
    }

    @Test
    fun fingerprintsStayWhatTheyWere() {
        // Documents are remembered by these: a change would forget every reading position. (Values
        // from the first version of the scheme.)
        val long = ByteArray(200_000) { (it % 251).toByte() }
        val short = ByteArray(1_000) { (it % 251).toByte() }
        assertEquals("6caa9d845c5241f43224ca33bc7dcc7d", DocumentStore.fingerprint(long))
        assertEquals("27fced0fcd13304e896beaab3088e259", DocumentStore.fingerprint(short))
    }

    @Test
    fun aFingerprintReadsOnlyBothEndsOfAFile() {
        val long = ByteArray(200_000) { (it % 251).toByte() }
        val reads = mutableListOf<Pair<Long, Int>>()
        val key = DocumentStore.fingerprint(long.size.toLong()) { offset, length ->
            reads += offset to length
            long.copyOfRange(offset.toInt(), offset.toInt() + length)
        }
        assertEquals(DocumentStore.fingerprint(long), key)
        assertEquals(listOf(0L to 65_536, 200_000L - 65_536 to 65_536), reads)

        // A file shorter than one such span is read once, whole.
        reads.clear()
        DocumentStore.fingerprint(1_000) { offset, length ->
            reads += offset to length
            ByteArray(length)
        }
        assertEquals(listOf(0L to 1_000), reads)
    }

    @Test
    fun fingerprintFollowsContentNotOrigin() {
        val a = ByteArray(300_000) { (it % 251).toByte() }
        assertEquals(DocumentStore.fingerprint(a), DocumentStore.fingerprint(a.copyOf()))
        // Different at the start, at the end, or in length.
        assertNotEquals(
            DocumentStore.fingerprint(a),
            DocumentStore.fingerprint(
                a.copyOf().also {
                    it[0] = 9
                },
            ),
        )
        assertNotEquals(
            DocumentStore.fingerprint(a),
            DocumentStore.fingerprint(
                a.copyOf().also {
                    it[it.lastIndex] = 9
                },
            ),
        )
        assertNotEquals(
            DocumentStore.fingerprint(a),
            DocumentStore.fingerprint(a.copyOf(a.size + 1)),
        )
        // Small files work too.
        assertNotEquals(
            DocumentStore.fingerprint(byteArrayOf(1, 2)),
            DocumentStore.fingerprint(byteArrayOf(1, 3)),
        )
        assertEquals(32, DocumentStore.fingerprint(ByteArray(0)).length)
    }

    @Test
    fun theFitIsKeptOnlyWhereItIsNotTheModesOwn() {
        val paged = view.copy(paged = true, fitWidth = true)
        DocumentStore(file).apply {
            opened("wide", "a.pdf", null, pageCount = 3)
            saveView("wide", paged)
            opened("usual", "b.pdf", null, pageCount = 3)
            saveView("usual", paged.copy(fitWidth = false))
            opened("scrolling", "c.pdf", null, pageCount = 3)
            saveView("scrolling", view.copy(fitWidth = true))
        }
        val again = DocumentStore(file)
        assertEquals(true, again.find("wide")?.view?.fitWidth)
        assertEquals(false, again.find("usual")?.view?.fitWidth)
        assertEquals(true, again.find("scrolling")?.view?.fitWidth)
        // Files written before there was a choice: paged fits the page, scrolling the width.
        val text = file.readText()
        assertEquals(1, Regex("fitWidth").findAll(text).count())
    }
}
