package net.xfbs.specto

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Decodes `crates/specto-ffi/fixtures/frame.bin`: the bytes Rust writes for a known frame
 * (`fixture_frame` in the crate's tests, which also checks that the file is current).
 *
 * If the layout changes on the Rust side, that test fails first, and this one fails if the Kotlin
 * decoder is not updated to match.
 */
class ViewFrameTest {
    private val frame: ViewFrame = run {
        val bytes = checkNotNull(javaClass.getResourceAsStream("/frame.bin")) {
            "fixture missing"
        }.readBytes()
        ViewFrame.decode(ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN), bytes.size)
    }

    @Test
    fun header() {
        assertFalse(frame.complete)
        assertTrue(frame.settled)
        assertTrue(frame.paged)
        assertTrue(frame.twoPages)
        assertFalse(frame.coverAlone)
        assertEquals(4, frame.currentPage)
        assertEquals(5, frame.lastPage)
        assertEquals(180, frame.settlesInMs)
        assertEquals(0.6645856618881226, frame.zoom, 0.0)
        assertEquals(1.744537353515625, frame.deviceScale, 0.0)
        assertEquals(1.5, frame.zoomFactor, 0.0)
        assertArrayEquals(longArrayOf(3, 9_000_000_000), frame.evicted)
    }

    @Test
    fun pagesAndTiles() {
        assertEquals(2, frame.pageCount)

        assertEquals(4, frame.page(0))
        assertEquals(21f, frame.pageLeft(0))
        assertEquals(-300f, frame.pageTop(0))
        assertEquals(1038f, frame.pageWidth(0))
        assertEquals(1469f, frame.pageHeight(0))
        assertEquals(0..1, frame.tiles(0))

        assertEquals(7L, frame.tileId(0))
        assertTrue(frame.tileIsExact(0))
        assertEquals(21f, frame.tileLeft(0))
        assertEquals(-300f, frame.tileTop(0))
        assertEquals(533f, frame.tileRight(0))
        assertEquals(212f, frame.tileBottom(0))

        // An id beyond 32 bits, and a fractional destination.
        assertEquals(1L shl 40, frame.tileId(1))
        assertFalse(frame.tileIsExact(1))
        assertEquals(533f, frame.tileLeft(1))
        assertEquals(996.5f, frame.tileRight(1))
        assertEquals(163.25f, frame.tileBottom(1))

        assertFalse(frame.pageIsBusy(0))

        // A page without tiles yet, and slow to draw.
        assertEquals(5, frame.page(1))
        assertEquals(1190f, frame.pageTop(1))
        assertTrue(frame.tiles(1).isEmpty())
        assertTrue(frame.pageIsBusy(1))
    }

    @Test
    fun thumbnails() {
        // Announced for pages 2 to 4. Page 2 is not rendered yet.
        assertEquals(0L, frame.thumbnailId(2))
        assertEquals(11L, frame.thumbnailId(3))
        assertEquals(12L, frame.thumbnailId(4))
        // Outside the announced range.
        assertEquals(0L, frame.thumbnailId(1))
        assertEquals(0L, frame.thumbnailId(5))
        assertTrue(frame.sameThumbnailsAs(frame))
        assertFalse(frame.sameThumbnailsAs(null))
    }

    @Test
    fun selection() {
        assertTrue(frame.selection)
        // After two search matches: two selected lines, the second slightly rotated.
        assertEquals(4, frame.highlightCount)
        assertEquals(ViewFrame.HIGHLIGHT_SELECTION, frame.highlightKind(2))
        assertEquals(ViewFrame.HIGHLIGHT_SELECTION, frame.highlightKind(3))
        assertEquals(100f, frame.highlightX(2, 0))
        assertEquals(80.5f, frame.highlightY(2, 0))
        assertEquals(900f, frame.highlightX(2, 2))
        assertEquals(50.25f, frame.highlightY(2, 2))
        assertEquals(60.5f, frame.highlightX(3, 3))
        assertEquals(90f, frame.highlightY(3, 3))
        // Only what is selected counts here, not the matches further down.
        assertArrayEquals(floatArrayOf(60f, 50.25f, 900f, 121f), frame.selectionBounds(), 0f)
        // It starts above the view and ends on it.
        assertEquals(null, frame.selectionStart)
        assertArrayEquals(floatArrayOf(300f, 121f, 300.5f, 91f), frame.selectionEnd, 0f)
    }

    @Test
    fun annotations() {
        val shapes = frame.shapes
        assertEquals(3, shapes.count)

        // A highlight on the first page: a quadrilateral that darkens.
        assertEquals(0, shapes.page(0))
        assertTrue(shapes.isFilled(0) && shapes.darkens(0) && shapes.isClosed(0))
        assertFalse(shapes.isStroked(0) || shapes.isRound(0))
        assertEquals(0xFFFFEB3B.toInt(), shapes.fill(0))
        assertEquals(100f, shapes.startX(0))
        assertEquals(200f, shapes.startY(0))
        assertEquals(0..2, shapes.segments(0))
        assertFalse(shapes.segmentIsCurve(1))
        assertEquals(300f, shapes.segmentX(1, 0))
        assertEquals(170.5f, shapes.segmentY(1, 0))

        // A line under some text on the second.
        assertEquals(1, shapes.page(1))
        assertTrue(shapes.isStroked(1))
        assertFalse(shapes.isFilled(1) || shapes.darkens(1) || shapes.isClosed(1))
        assertEquals(0xFFE01C24.toInt(), shapes.stroke(1))
        assertEquals(2.5f, shapes.strokeWidth(1))
        assertEquals(3..3, shapes.segments(1))
        assertEquals(400f, shapes.segmentX(3, 0))

        // A drawing: filled and outlined with a round pen, with a curve.
        assertTrue(shapes.isFilled(2) && shapes.isStroked(2) && shapes.isRound(2))
        assertEquals(0x8000FF00.toInt(), shapes.fill(2))
        assertEquals(4..5, shapes.segments(2))
        assertTrue(shapes.segmentIsCurve(4))
        assertEquals(520f, shapes.segmentX(4, 0))
        assertEquals(560f, shapes.segmentX(4, 1))
        assertEquals(580f, shapes.segmentX(4, 2))
        assertEquals(1400f, shapes.segmentY(4, 2))
        assertFalse(shapes.segmentIsCurve(5))
        assertEquals(1450f, shapes.segmentY(5, 0))

        // One on the second of the thumbnails, which is page 3.
        val small = frame.thumbnailShapes
        assertEquals(1, small.count)
        assertEquals(1, small.page(0))
        assertEquals(1, frame.thumbnailSlot(3))
        assertTrue(small.isFilled(0) && small.darkens(0) && small.isClosed(0))
        assertEquals(120f, small.startX(0))
        assertEquals(0..2, small.segments(0))
        assertEquals(480.5f, small.segmentX(0, 0))
        assertEquals(215f, small.segmentY(2, 0))
        assertTrue(frame.sameThumbnailsAs(frame))

        // The highlight is picked.
        assertArrayEquals(
            floatArrayOf(98f, 202f, 302f, 202f, 302f, 168.5f, 98f, 168.5f),
            frame.picked,
            0f,
        )
        assertArrayEquals(floatArrayOf(98f, 168.5f, 302f, 202f), frame.pickedBounds(), 0f)
    }

    @Test
    fun searchMatches() {
        assertEquals(ViewFrame.HIGHLIGHT_MATCH, frame.highlightKind(0))
        assertEquals(ViewFrame.HIGHLIGHT_CURRENT_MATCH, frame.highlightKind(1))
        assertEquals(400f, frame.highlightX(0, 0))
        assertEquals(300f, frame.highlightY(0, 0))
        assertEquals(480f, frame.highlightX(1, 2))
        assertEquals(470f, frame.highlightY(1, 2))
    }

    @Test
    fun handles() {
        // The end handle hangs below the end of the last line, to the right.
        val edge = frame.selectionEnd!!
        assertEquals(330f to 151f, handleCenter(edge, end = true, radius = 30f))
        assertEquals(270f to 151f, handleCenter(edge, end = false, radius = 30f))

        // A touch on it takes hold of the end. The selection then follows the middle of the line's
        // edge, not the finger below it.
        val grab = frame.handleAt(340f, 160f, radius = 30f, reach = 80f)!!
        assertTrue(grab.end)
        assertEquals(300.25f - 340f, grab.dx)
        assertEquals(106f - 160f, grab.dy)
        // Near enough still counts, and further away is no handle.
        assertTrue(frame.handleAt(330f, 151f + 79f, radius = 30f, reach = 80f)!!.end)
        assertEquals(null, frame.handleAt(330f, 151f + 81f, radius = 30f, reach = 80f))
        assertEquals(null, frame.handleAt(100f, 60f, radius = 30f, reach = 80f))
    }
}
