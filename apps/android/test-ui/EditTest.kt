package net.xfbs.specto

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.net.Uri
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.test.withKeyDown
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Editing the pages: picking them in the page grid, turning and deleting them, undo and redo, and
 * saving the result as a new document.
 *
 * Where "Save as" writes is asked of the system's file dialog, which never opens here: the request
 * is caught on its way out and answered with a file in the app's cache.
 */
@RunWith(AndroidJUnit4::class)
class EditTest : AppTest() {
    private val saved: File get() = File(instrumentation.targetContext.cacheDir, "edited.pdf")

    /** Answers the question where to save with [target], and counts the times it was asked. */
    private inner class SaveDialogs : Instrumentation.ActivityMonitor() {
        @Volatile
        var target: File = saved

        @Volatile
        var asked = 0

        /** The name the app suggested the last time. */
        @Volatile
        var suggested: String? = null

        override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
            if (intent.action != Intent.ACTION_CREATE_DOCUMENT) return null
            asked++
            suggested = intent.getStringExtra(Intent.EXTRA_TITLE)
            return Instrumentation.ActivityResult(
                Activity.RESULT_OK,
                Intent().setData(Uri.fromFile(target)),
            )
        }
    }

    private val dialogs = SaveDialogs()

    @Before
    fun catchSaveDialogs() {
        saved.delete()
        instrumentation.addMonitor(dialogs)
    }

    @After
    fun letSaveDialogsThrough() {
        instrumentation.removeMonitor(dialogs)
        saved.delete()
    }

    private fun openPages() = open("pages.pdf", shows = "1 / 12")

    /** The page grid: beside the document on a wide window, in its place otherwise. */
    private fun showGrid() {
        if (!wide) compose.onNodeWithContentDescription("Contents and pages").performClick()
        waitForText("2")
    }

    private fun leaveGrid() {
        if (!wide) compose.onNodeWithContentDescription("Back to document").performClick()
    }

    /** Picks the page numbered [label] in the grid, the way picking starts: with a long press. */
    private fun pick(label: String) {
        compose.onNodeWithText(label).performTouchInput { longClick() }
        compose.waitForIdle()
    }

    private fun shown(text: String): Boolean =
        compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    /** Turns page 2, so that there is something unsaved. */
    private fun edit() {
        showGrid()
        pick("2")
        compose.onNodeWithContentDescription("Rotate right").performClick()
        compose.onNodeWithContentDescription("Clear selection").performClick()
        leaveGrid()
        waitForText("Edited")
    }

    @Test
    fun pagesArePickedInTheGrid() {
        openPages()
        showGrid()
        // A tap goes to a page. A long press picks it, and from then on a tap picks more, or lets
        // go again.
        pick("2")
        waitForText("1 selected")
        compose.onNodeWithText("2").assertIsSelected()
        compose.onNodeWithText("3").performClick()
        waitForText("2 selected")
        compose.onNodeWithText("3").assertIsSelected()
        compose.onNodeWithText("3").performClick()
        waitForText("1 selected")
        // Still on the first page: none of that went anywhere.
        leaveGridKeepingPicks()

        // Back lets go of them, and only then leaves the grid.
        Espresso.pressBack()
        waitUntilGone("1 selected")
        compose.onNodeWithText("2").assertIsDisplayed()

        // Everything at once, which can't all be deleted.
        pick("2")
        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Select all").performClick()
        waitForText("12 selected")
        compose.onNodeWithContentDescription("Delete pages").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Clear selection").performClick()
        waitUntilGone("12 selected")
    }

    /** On a wide window the document is beside the grid, and shows that the page didn't change. */
    private fun leaveGridKeepingPicks() {
        if (wide) compose.onNodeWithText("1 / 12").assertIsDisplayed()
    }

    @Test
    fun pagesAreTurnedAndDeletedAndItCanBeUndone() {
        openPages()
        showGrid()
        pick("2")
        compose.onNodeWithContentDescription("Rotate right").performClick()
        // They stay picked, to be turned further.
        compose.onNodeWithText("1 selected").assertIsDisplayed()
        compose.onNodeWithContentDescription("Clear selection").performClick()
        waitForText("Edited")
        // Page 2 lies on its side now.
        menu("Properties")
        waitForText("210 × 297 mm (A4); other pages differ")
        compose.onNodeWithText("Close").performClick()

        pick("3")
        compose.onNodeWithContentDescription("Delete pages").performClick()
        waitUntilGone("1 selected")
        leaveGrid()
        waitForText("1 / 11")

        // Step by step back to the document as it was, and forward again.
        menu("Undo delete")
        waitForText("1 / 12")
        menu("Undo rotate")
        waitUntilGone("Edited")
        compose.onNodeWithContentDescription("More").performClick()
        compose.onAllNodesWithText("Undo rotate").assertCountEquals(0)
        compose.onNodeWithText("Redo rotate").performClick()
        waitForText("Edited")
        // With a keyboard: Ctrl+Z, and Shift+Ctrl+Z or Ctrl+Y.
        page.performKeyInput { withKeyDown(Key.CtrlLeft) { pressKey(Key.Z) } }
        waitUntilGone("Edited")
        page.performKeyInput { withKeyDown(Key.CtrlLeft) { pressKey(Key.Y) } }
        waitForText("Edited")
        page.performKeyInput {
            withKeyDown(Key.CtrlLeft) { withKeyDown(Key.ShiftLeft) { pressKey(Key.Z) } }
        }
        waitForText("1 / 11")
    }

    /** Where the thumbnail labeled [label] is in the page grid. */
    private fun cell(label: String): Rect {
        val grid = compose.onNodeWithTag(PAGES_GRID_TAG).fetchSemanticsNode().boundsInRoot
        return compose.onNodeWithText(
            label,
        ).fetchSemanticsNode().boundsInRoot.translate(-grid.topLeft)
    }

    /** The labels given, in the order their thumbnails are in: row by row, or down the sidebar. */
    private fun inOrder(vararg labels: String): List<String> =
        labels.sortedWith(compareBy({ cell(it).top }, { cell(it).left }))

    /**
     * Presses long on the thumbnail labeled [from] and drags it to just behind the one labeled
     * [behind].
     */
    private fun drag(from: String, behind: String) {
        val start = cell(from).center
        val target = cell(behind)
        // Behind is to the right in the grid, and below in the sidebar's single column.
        val end = if (wide) {
            Offset(
                target.center.x,
                target.bottom - 4f,
            )
        } else {
            Offset(target.right - 4f, target.center.y)
        }
        compose.onNodeWithTag(PAGES_GRID_TAG).performTouchInput {
            down(start)
            advanceEventTime(viewConfiguration.longPressTimeoutMillis + 100)
            moveTo((start + end) / 2f)
            moveTo(end)
            up()
        }
        compose.waitForIdle()
    }

    @Test
    fun draggingMovesPages() {
        // Its first pages are numbered i, ii and iii, so they can be told apart.
        open("book.pdf", shows = "i (1 / 14)")
        if (!wide) compose.onNodeWithContentDescription("Contents and pages").performClick()
        waitForText("iii")
        assertEquals(listOf("i", "ii", "iii"), inOrder("i", "ii", "iii"))

        // A long press that moves on takes the page along, and it stays picked.
        drag("ii", behind = "iii")
        waitForText("1 selected")
        compose.waitUntil(WAIT_MS) { inOrder("i", "ii", "iii") == listOf("i", "iii", "ii") }
        compose.onNodeWithText("ii").assertIsSelected()
        // Dragging one of several picked pages moves them all, in their order.
        compose.onNodeWithText("i").performClick()
        waitForText("2 selected")
        drag("i", behind = "iii")
        compose.waitUntil(WAIT_MS) { inOrder("i", "ii", "iii") == listOf("iii", "i", "ii") }
        compose.onNodeWithText("2 selected").assertIsDisplayed()

        // A long press on a picked page that goes nowhere lets go of it.
        pick("i")
        waitForText("1 selected")
        compose.onNodeWithContentDescription("Clear selection").performClick()
        // Each drag was one edit.
        menu("Undo move")
        compose.waitUntil(WAIT_MS) { inOrder("i", "ii", "iii") == listOf("i", "iii", "ii") }
        menu("Undo move")
        compose.waitUntil(WAIT_MS) { inOrder("i", "ii", "iii") == listOf("i", "ii", "iii") }
        waitUntilGone("Edited")
    }

    @Test
    fun savingAsWritesTheEditedDocumentAndShowsIt() {
        openPages()
        showGrid()
        pick("2")
        compose.onNodeWithContentDescription("Delete pages").performClick()
        leaveGrid()
        waitForText("1 / 11")
        page.performKeyInput { pressKey(Key.DirectionRight) }
        waitForText("2 / 11")

        menu("Save as")
        // The saved document takes this one's place, where the reader was.
        waitForText("edited.pdf")
        waitForText("2 / 11")
        assertEquals(
            "asked once, suggesting the document's name",
            1 to "pages.pdf",
            dialogs.asked to dialogs.suggested,
        )
        assertTrue("written", saved.length() > 1000)
        // It is a document of its own: nothing unsaved, nothing to undo.
        assertTrue(!shown("Edited"))
        compose.onNodeWithContentDescription("More").performClick()
        compose.onAllNodesWithText("Undo delete").assertCountEquals(0)
        compose.onNodeWithText("Close").performClick()
        // Both are offered again. The original has all its pages still (and remembers where it was
        // left, like any document).
        waitForText("Recent")
        compose.onNodeWithText("edited.pdf").assertIsDisplayed()
        compose.onNodeWithText("pages.pdf").performClick()
        waitForText("2 / 12")
    }

    @Test
    fun savingOverTheOpenDocument() {
        openPages()
        showGrid()
        pick("2")
        compose.onNodeWithContentDescription("Delete pages").performClick()
        leaveGrid()
        waitForText("1 / 11")
        // The reader picks the very file that is open.
        val opened = File(instrumentation.targetContext.cacheDir, "pages.pdf")
        val before = opened.length()
        dialogs.target = opened
        menu("Save as")
        waitUntilGone("Edited")
        waitForText("1 / 11")
        compose.onNodeWithText("pages.pdf").assertIsDisplayed()
        assertTrue("written anew", opened.length() != before)
        // Read afresh from what was written: nothing to undo, and it opens like that from now on.
        compose.onNodeWithContentDescription("More").performClick()
        compose.onAllNodesWithText("Undo delete").assertCountEquals(0)
        compose.onNodeWithText("Close").performClick()
        waitForText("Recent")
        compose.onNodeWithText("pages.pdf").performClick()
        waitForText("1 / 11")
    }

    @Test
    fun aDocumentFromAnotherAppWaitsForTheAnswer() {
        openPages()
        edit()
        // Another app asks for a document to be shown.
        val target = instrumentation.targetContext
        File(target.cacheDir, "book.pdf").outputStream().use { out ->
            instrumentation.context.assets.open("book.pdf").use { it.copyTo(out) }
        }
        val other = Intent(Intent.ACTION_VIEW)
            .setDataAndType(Uri.fromFile(File(target.cacheDir, "book.pdf")), "application/pdf")
        scenario!!.onActivity { instrumentation.callActivityOnNewIntent(it, other) }
        waitForText("Save changes?")
        compose.onNodeWithText("Cancel").performClick()
        waitUntilGone("Save changes?")
        // Still here, edits and all.
        compose.onNodeWithText("pages.pdf").assertIsDisplayed()
        compose.onNodeWithText("Edited").assertIsDisplayed()

        scenario!!.onActivity { instrumentation.callActivityOnNewIntent(it, other) }
        waitForText("Save changes?")
        compose.onNodeWithText("Discard").performClick()
        waitForText("book.pdf")
        assertTrue(!shown("Edited"))
    }

    @Test
    fun savingWritesOverTheFileAndKeepsTheDocumentOpen() {
        openPages()
        // Nothing to save yet.
        compose.onNodeWithContentDescription("More").performClick()
        compose.onAllNodesWithText("Save").assertCountEquals(0)
        compose.onNodeWithText("Save as").assertIsDisplayed()
        Espresso.pressBack()

        showGrid()
        pick("2")
        compose.onNodeWithContentDescription("Delete pages").performClick()
        leaveGrid()
        waitForText("1 / 11")
        val opened = File(instrumentation.targetContext.cacheDir, "pages.pdf")
        val before = opened.readBytes()
        menu("Save")
        waitUntilGone("Edited")
        val saved = opened.readBytes()
        assertTrue("written anew", !saved.contentEquals(before))
        assertEquals("a PDF", "%PDF-", String(saved, 0, 5))
        // Still the same document, where it was: the edit can be taken back, which is a change to
        // save again. Ctrl+S does that.
        compose.onNodeWithText("1 / 11").assertIsDisplayed()
        menu("Undo delete")
        waitForText("1 / 12")
        compose.onNodeWithText("Edited").assertIsDisplayed()
        page.performKeyInput { withKeyDown(Key.CtrlLeft) { pressKey(Key.S) } }
        waitUntilGone("Edited")
        assertTrue("written again", !opened.readBytes().contentEquals(saved))
        assertEquals("never asked where", 0, dialogs.asked)

        // Saved, it is left without a question, and listed once.
        menu("Close")
        waitForText("Recent")
        compose.onNodeWithText("pages.pdf").performClick()
        waitForText("1 / 12")
        assertTrue(!shown("Edited"))
    }

    @Test
    fun leavingUnsavedEditsAsksFirst() {
        openPages()
        edit()
        menu("Close")
        waitForText("Save changes?")
        compose.onNodeWithText("Cancel").performClick()
        waitUntilGone("Save changes?")
        compose.onNodeWithText("Edited").assertIsDisplayed()

        // Saved from the question, the reader goes on to where they were going.
        menu("Close")
        waitForText("Save changes?")
        compose.onNodeWithText("Save").performClick()
        waitForText("Recent")
        compose.onNodeWithText("pages.pdf").performClick()
        waitForText("1 / 12")
        assertTrue(!shown("Edited"))
        menu("Properties")
        waitForText("210 × 297 mm (A4); other pages differ")
        compose.onNodeWithText("Close").performClick()

        // Discarded, the edits are gone: page 2 is turned once, not twice.
        edit()
        menu("Close")
        waitForText("Save changes?")
        compose.onNodeWithText("Discard").performClick()
        waitForText("Recent")
        compose.onNodeWithText("pages.pdf").performClick()
        waitForText("1 / 12")
        assertTrue(!shown("Edited"))
        menu("Properties")
        waitForText("210 × 297 mm (A4); other pages differ")
        assertEquals(0, dialogs.asked)
    }

    @Test
    fun aFileThatCantBeWrittenOverIsSavedElsewhere() {
        openPages()
        val opened = File(instrumentation.targetContext.cacheDir, "pages.pdf")
        try {
            // As most documents handed over by other apps are: to read only.
            assertTrue(opened.setReadOnly())
            reopen("pages.pdf")
            waitForText("1 / 12")
            edit()
            compose.onNodeWithContentDescription("More").performClick()
            compose.onAllNodesWithText("Save").assertCountEquals(0)
            // Leaving it, the question offers the only way there is.
            compose.onNodeWithText("Close").performClick()
            waitForText("Save changes?")
            compose.onNodeWithText("Save as").performClick()
            waitForText("Recent")
            compose.onNodeWithText("edited.pdf").assertIsDisplayed()
            compose.onNodeWithText("pages.pdf").assertIsDisplayed()
            assertEquals(1, dialogs.asked)
        } finally {
            opened.setWritable(true)
        }
    }

    @Test
    fun unsavedEditsOutliveTheApp() {
        openPages()
        edit()
        // The system ends the activity in the background and brings it back.
        scenario!!.recreate()
        waitForText("Edited")
        compose.onNodeWithText("pages.pdf").assertIsDisplayed()
        // The edits are back, but not how they came about.
        compose.onNodeWithContentDescription("More").performClick()
        compose.onAllNodesWithText("Undo rotate").assertCountEquals(0)
        Espresso.pressBack()
        menu("Properties")
        waitForText("210 × 297 mm (A4); other pages differ")
        compose.onNodeWithText("Close").performClick()

        // Also when the document is opened anew.
        reopen("pages.pdf")
        waitForText("Edited")

        // Until they are discarded, which is asked first.
        menu("Discard changes")
        waitForText("Discard changes?")
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Edited").assertIsDisplayed()
        menu("Discard changes")
        compose.onNodeWithText("Discard").performClick()
        waitUntilGone("Edited")
        menu("Properties")
        waitForText("210 × 297 mm (A4)")
        assertTrue(!shown("210 × 297 mm (A4); other pages differ"))
        compose.onNodeWithText("Close").performClick()
        reopen("pages.pdf")
        waitForText("1 / 12")
        assertTrue(!shown("Edited"))
    }

    @Test
    fun withoutEditsNothingIsAsked() {
        openPages()
        menu("Close")
        waitForText("Recent")
        assertTrue(!shown("Save changes?"))
    }
}
