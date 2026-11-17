package net.xfbs.specto

import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.util.LongSparseArray
import androidx.compose.animation.core.AnimationState
import androidx.compose.animation.core.DecayAnimationSpec
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateDecay
import androidx.compose.animation.core.tween
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.pow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.specto_ffi.Activated
import uniffi.specto_ffi.DocumentInfo
import uniffi.specto_ffi.DraftStore
import uniffi.specto_ffi.EditState
import uniffi.specto_ffi.FieldTap
import uniffi.specto_ffi.Markup
import uniffi.specto_ffi.OutlineEntry
import uniffi.specto_ffi.PageSize
import uniffi.specto_ffi.PdfFile
import uniffi.specto_ffi.SavedView
import uniffi.specto_ffi.SearchStatus
import uniffi.specto_ffi.ViewListener
import uniffi.specto_ffi.ViewMode
import uniffi.specto_ffi.ViewSession

private const val TAG = "specto"

private fun newBuffer(bytes: Int): ByteBuffer =
    ByteBuffer.allocateDirect(bytes).order(ByteOrder.nativeOrder())

/** How far a double tap zooms in, relative to the fitted view. */
private const val DOUBLE_TAP_ZOOM = 2.5

/** How much of the view's height Page Down scrolls. */
private const val SCREENFUL = 0.9f

/** How far an arrow key scrolls, in dp. */
private const val LINE_DP = 60f

/** Zoom factor of one Ctrl and plus. */
private const val ZOOM_STEP = 1.25f

/** Duration of the slide that brings a swiped page to rest. */
private const val SLIDE_MS = 260

/**
 * One open document: the Rust [ViewSession] (layout, zoom, scrolling, tile planning and rendering)
 * plus the tile bitmaps it tells us to keep.
 *
 * Whatever happens every frame goes through [Native] (plain JNI). The generated UniFFI bindings are
 * only used for occasional calls.
 *
 * Main thread only, except [tilesReady]. Positions and distances are in dp unless noted. Call
 * [close] when done.
 *
 * Throws `PdfException` if the file can't be opened, such as `PdfException.PasswordRequired` for a
 * missing or wrong [password].
 */
class PdfViewState(
    file: PdfFile,
    password: String?,
    cacheBytes: Long,
    /** Where the reader left off last time, to continue there. */
    restore: SavedView? = null,
    /** Whether the window has room for two pages side by side (see [wideWindow]). */
    wideWindow: Boolean = false,
    sidebarOpen: Boolean = true,
) : AutoCloseable {
    private val main = Handler(Looper.getMainLooper())
    private val tilesPosted = AtomicBoolean(false)
    private val searchPosted = AtomicBoolean(false)
    private var closed = false

    private val listener = object : ViewListener {
        // Called on a Rust render thread.
        override fun tilesReady() {
            if (tilesPosted.compareAndSet(false, true)) {
                main.post {
                    tilesPosted.set(false)
                    collectTiles()
                }
            }
        }

        // Called on the thread that reads the document for searching.
        override fun searchChanged() {
            if (searchPosted.compareAndSet(false, true)) {
                main.post {
                    searchPosted.set(false)
                    searchMoved()
                }
            }
        }
    }
    private val session = ViewSession.openFile(file, password, cacheBytes.toULong(), listener)
    private val sessionId = session.id().toLong()
    private val bitmaps = LongSparseArray<Bitmap>()
    private var buffer = newBuffer(16 * 1024)
    private var motion: Job? = null
    private val settle = Runnable { refresh() }
    private var thumbnailOwner: Any? = null
    private var pendingRestore: SavedView? = restore

    /** Width and height of the view in dp. */
    private var viewport = Offset.Zero

    /** Device pixels per dp. */
    private var density = 1f

    // What follows describes the pages as they are now: editing changes it (see "editing" below).

    /** Width and height of each page as displayed, in points (1/72 inch). */
    var pageSizes: List<PageSize> by mutableStateOf(session.pageSizes())
        private set

    val pageCount: Int get() = pageSizes.size

    /**
     * Width divided by height of [page] as displayed, or that of a sheet of paper for a page that
     * isn't there any more.
     */
    fun pageAspect(page: Int): Float =
        pageSizes.getOrNull(page)?.let { it.width / it.height } ?: 0.707f

    /** What the document says about itself: title, author, dates and so on. */
    fun info(): DocumentInfo = session.info()

    /**
     * The pages' printed numbers, for documents that number them other than 1, 2, 3 (roman numerals
     * for the front matter, "A-1" in an appendix, and so on).
     */
    private var pageLabels: List<String>? by mutableStateOf(session.pageLabels())

    /** Whether pages have printed numbers that differ from their positions. */
    val hasPageLabels: Boolean get() = pageLabels != null

    /** What [page] (zero-based) is called: its printed number, else its position. */
    fun pageLabel(page: Int): String = pageLabels?.getOrNull(page) ?: "${page + 1}"

    /** The zero-based page meant by [text]: a printed number first, else a position. */
    fun findPage(text: String): Int? = session.findPage(text)?.toInt()

    /** The outline (table of contents) in reading order, or empty if there is none. */
    var outline: List<OutlineEntry> by mutableStateOf(session.outline())
        private set

    init {
        // On a phone, swiping through fitted pages is the default.
        session.setViewMode(ViewMode.PAGED)
    }

    /**
     * One page at a time, swiped left and right, or else one continuously scrolling column. Keeps
     * the current page.
     */
    var paged: Boolean
        get() = pagedNow
        set(value) {
            if (value == paged) return
            stopMotion()
            session.setViewMode(if (value) ViewMode.PAGED else ViewMode.CONTINUOUS)
            // Each mode starts with its own fit.
            fitWidth = !value
            refresh()
        }

    /**
     * What the view is fitted to unless the reader zoomed in: the page's width, the rest of a
     * taller page being reached by scrolling, or else the whole page.
     *
     * Pages start out whole when [paged] and fitted to the width when scrolling. Setting it also
     * undoes any zooming.
     */
    var fitWidth: Boolean by mutableStateOf(restore?.fitWidth ?: false)
        private set

    fun fit(width: Boolean) {
        stopMotion()
        session.setFitWidth(width)
        fitWidth = width
        refresh()
    }

    /** Follows the frames, but only changes when the mode does: the top bar reads it. */
    private var pagedNow by mutableStateOf(true)

    // Search
    //
    // Like the selection, a search lives in Rust and shows up in frames, as highlighted matches.
    // The first one has the whole document read in the background, so it keeps finding more for a
    // moment.

    /**
     * How the search stands: matches so far, the current one, how much has been searched. Null
     * without a search.
     */
    var searchStatus: SearchStatus? by mutableStateOf(null)
        private set

    /**
     * Searches for [text], ignoring case and accents, and goes to the first match from the current
     * page on.
     *
     * Fine to call for every letter typed. Blank text ends the search.
     */
    fun search(text: String) {
        if (closed) return
        stopMotion()
        session.search(text)
        searchMoved()
    }

    /** Goes to the next (or previous) match. After the last one comes the first again. */
    fun searchStep(forward: Boolean) {
        if (closed) return
        stopMotion()
        session.searchStep(forward)
        searchMoved()
    }

    fun endSearch() {
        if (closed) return
        session.endSearch()
        searchMoved()
    }

    private fun searchMoved() {
        if (closed) return
        searchStatus = session.searchStatus()
        refresh()
    }

    // Editing
    //
    // Pages can be turned, removed and moved, and every such edit can be undone. Nothing touches
    // the file until the document is written somewhere ([writeTo]). Pages are named by their
    // position, from 0.

    /** Whether the document has unsaved edits, and what undo and redo would do. */
    var editState: EditState by mutableStateOf(session.editState())
        private set

    /**
     * The pages picked in the page grid to act on, by position.
     *
     * Kept here rather than in the grid, since the bar above it acts on them.
     */
    var selectedPages: Set<Int> by mutableStateOf(emptySet())

    /** Picks [page], or lets go of it if it was picked. */
    fun togglePage(page: Int) {
        selectedPages = if (page in selectedPages) selectedPages - page else selectedPages + page
    }

    /** Turns [pages] by a quarter. They stay picked. */
    fun rotatePages(pages: Collection<Int>, clockwise: Boolean) {
        if (closed) return
        if (session.rotatePages(pages.map { it.toUInt() }, clockwise)) pagesChanged()
    }

    /**
     * Removes [pages].
     *
     * A document keeps at least one page. Nothing is picked afterwards.
     */
    fun deletePages(pages: Collection<Int>) {
        if (closed) return
        if (session.deletePages(pages.map { it.toUInt() })) {
            selectedPages = emptySet()
            pagesChanged()
        }
    }

    /**
     * Moves [pages], in their order in the document, to before the page now at [to] (the page
     * count: to the end). They stay picked, where they are then.
     */
    fun movePages(pages: Collection<Int>, to: Int) {
        if (closed) return
        val first = session.movePages(pages.map { it.toUInt() }, to.toUInt())?.toInt() ?: return
        if (selectedPages.isNotEmpty()) {
            selectedPages = (first until first + pages.toSet().size).toSet()
        }
        pagesChanged()
    }

    /** Takes back the last edit, if there is one. */
    fun undo() {
        if (closed) return
        if (session.undo() != null) {
            selectedPages = emptySet()
            pagesChanged()
        }
    }

    /** Does again what [undo] took back. */
    fun redo() {
        if (closed) return
        if (session.redo() != null) {
            selectedPages = emptySet()
            pagesChanged()
        }
    }

    /**
     * Keeps the unsaved edits in [drafts] as the draft of [key] (the document's key in the
     * [DocumentStore]), or deletes the draft if there are none.
     *
     * For when the app goes into the background, where the system may end it. Throws
     * `PdfException` if the draft can't be written.
     */
    fun keepDraft(drafts: DraftStore, key: String) {
        if (closed) return
        session.keepDraft(drafts, key)
    }

    /**
     * Takes over the draft of [key] in [drafts], if there is one: the edits are back, with nothing
     * to undo. Returns whether there was one.
     *
     * Throws `PdfException` if there was one that can't be taken over, which is deleted then.
     */
    fun restoreDraft(drafts: DraftStore, key: String): Boolean {
        if (closed || !session.restoreDraft(drafts, key)) return false
        pagesChanged()
        return true
    }

    /** Positions changed: reads again what goes by them. */
    private fun pagesChanged() {
        stopMotion()
        // An edit lets go of a picked annotation.
        hasPickedAnnotation = false
        pageSizes = session.pageSizes()
        pageLabels = session.pageLabels()
        outline = session.outline()
        editState = session.editState()
        // An edit drops the text selection and runs a search again.
        searchStatus = session.searchStatus()
        refresh()
    }

    /**
     * Writes the document as edited, as a new PDF, into the open file descriptor [fd], which is
     * closed either way.
     *
     * With [countsAsSaved] the document has no unsaved edits afterwards. Without, this is a copy on
     * the side. Throws `PdfException.Save` if it can't be written.
     */
    suspend fun writeTo(fd: Int, countsAsSaved: Boolean) {
        withContext(Dispatchers.IO) { session.saveToFd(fd, countsAsSaved) }
        if (!closed) editState = session.editState()
    }

    /**
     * Writes the document as edited to [fd] for the system's print service: like [writeTo], but
     * without its password protection, and counting for nothing. Throws `PdfException.Save` if it
     * can't be written.
     */
    suspend fun printTo(fd: Int) = withContext(Dispatchers.IO) { session.printToFd(fd) }

    /**
     * The copy last written with [writeTo] has been put where the document is kept: what it holds
     * is saved now. Edits made since are not.
     */
    fun writtenIsSaved() {
        if (closed) return
        session.writtenIsSaved()
        editState = session.editState()
    }

    // Annotations
    //
    // Marks on the text, and what a file brings along. They live in Rust. A frame has what is to be
    // drawn of them, and the box around the one that is picked.

    /** Whether an annotation is picked (by a tap on it), on screen or not. */
    var hasPickedAnnotation: Boolean by mutableStateOf(false)
        private set

    /** Marks the selected text, which is then no longer selected. False if there was none. */
    fun markSelection(markup: Markup): Boolean {
        if (closed || !session.markSelection(markup)) return false
        editState = session.editState()
        refresh()
        return true
    }

    /**
     * A tap at [at]: picks the annotation there, and lets go of one picked before. Returns whether
     * one is picked now.
     */
    fun pickAnnotation(at: Offset): Boolean {
        if (closed) return false
        val had = hasPickedAnnotation
        hasPickedAnnotation = session.pickAnnotationAt(at.x.toDouble(), at.y.toDouble())
        if (had || hasPickedAnnotation) refresh()
        return hasPickedAnnotation
    }

    fun clearPickedAnnotation() {
        if (closed || !hasPickedAnnotation) return
        session.clearPickedAnnotation()
        hasPickedAnnotation = false
        refresh()
    }

    /**
     * Deletes the picked annotation.
     *
     * Throws `PdfException.Save` for one of a file whose page can't be written without it.
     */
    fun deletePickedAnnotation() {
        if (closed || !hasPickedAnnotation) return
        try {
            session.removePickedAnnotation()
        } finally {
            hasPickedAnnotation = false
            session.clearPickedAnnotation()
            editState = session.editState()
            refresh()
        }
    }

    // Forms
    //
    // The fields of the form live in Rust. What is filled in shows in the pages as rendered.

    /**
     * The field that a tap asked to fill in: a text field to type into ([FieldTap.Text]), or a menu
     * or list to choose from ([FieldTap.Choose], [FieldTap.ChooseSeveral]). Null if none is.
     *
     * A frame has the box around it. Follow with [setFieldText], [chooseOption], [chooseOptions] or
     * [endField].
     */
    var askedField: FieldTap? by mutableStateOf(null)
        private set

    /**
     * A tap at [at] on the form: ticks or unticks a checkbox, chooses a radio button, or asks for
     * what goes into a text field, a menu or a list ([askedField]).
     *
     * A picked annotation is let go of. Returns whether a field that can be filled in was there.
     */
    fun tapField(at: Offset): Boolean {
        if (closed) return false
        val tap = session.tapField(at.x.toDouble(), at.y.toDouble())
        if (tap == FieldTap.Nothing) return false
        hasPickedAnnotation = false
        if (tap == FieldTap.Changed) {
            editState = session.editState()
        } else {
            askedField = tap
        }
        refresh()
        return true
    }

    /**
     * Fills [text] into the text field that was asked about, as far as the field takes it, or into
     * the menu that was asked about and takes text of one's own.
     */
    fun setFieldText(text: String) {
        val field = when (val asked = askedField) {
            is FieldTap.Text -> asked.field
            is FieldTap.Choose -> asked.field
            else -> null
        }
        if (!closed && field != null) session.setFieldText(field, text)
        endField()
    }

    /** Chooses the options [chosen] of the list that was asked about, and no others. */
    fun chooseOptions(chosen: Set<Int>) {
        val asked = askedField as? FieldTap.ChooseSeveral
        if (!closed && asked != null) {
            session.chooseOptions(asked.field, chosen.sorted().map { it.toUInt() })
        }
        endField()
    }

    /** Chooses option [index] of the menu or list that was asked about. */
    fun chooseOption(index: Int) {
        val asked = askedField as? FieldTap.Choose
        if (!closed && asked != null) session.chooseOption(asked.field, index.toUInt())
        endField()
    }

    /** Lets go of the field that was asked about, filled in or not. */
    fun endField() {
        askedField = null
        if (closed) return
        session.endField()
        editState = session.editState()
        refresh()
    }

    /** Whether the document has a form with a field that can be filled in. */
    val hasForm: Boolean by lazy { session.hasForm() }

    /** Whether every field that can be filled in is tinted, to show where they are. */
    var highlightFields: Boolean by mutableStateOf(false)
        private set

    /** Tints the fields that can be filled in, or stops doing so. */
    fun toggleHighlightFields() {
        if (closed) return
        highlightFields = !highlightFields
        session.setHighlightFields(highlightFields)
        refresh()
    }

    /** Sets every field back to what the form has for it at first, as one edit. */
    fun resetForm() {
        if (closed || !session.resetForm()) return
        editState = session.editState()
        refresh()
    }

    // Text selection
    //
    // The selection lives in Rust and shows up in frames: the highlights to draw and where its two
    // handles go.

    /** Whether any text is selected, on screen or not. */
    var hasSelection: Boolean by mutableStateOf(false)
        private set

    /** A long press at [at]: selects the word there. False if there is no text there. */
    fun selectWordAt(at: Offset): Boolean {
        if (closed || !session.selectWordAt(at.x.toDouble(), at.y.toDouble())) return false
        // One thing at a time is acted on: the text, or an annotation.
        if (hasPickedAnnotation) {
            session.clearPickedAnnotation()
            hasPickedAnnotation = false
        }
        stopMotion()
        refresh()
        return true
    }

    /**
     * A handle was touched: [selectTo] now moves that end of the selection (its end, or else its
     * start) and the other one stays.
     */
    fun grabSelection(end: Boolean) {
        if (!closed) session.grabSelection(end)
    }

    /**
     * Extends the selection to the text nearest to [at]: word by word from the word a long press
     * selected, or moving the end last grabbed.
     */
    fun selectTo(at: Offset) {
        Native.selectTo(sessionId, at.x.toDouble(), at.y.toDouble())
        refresh()
    }

    fun selectAll() {
        if (closed) return
        session.selectAll()
        refresh()
    }

    fun clearSelection() {
        if (closed || !hasSelection) return
        session.clearSelection()
        refresh()
    }

    /**
     * The selected text, for copying, or null if there is none.
     *
     * Reads every selected page, which takes a moment for a whole long document, so not on the main
     * thread.
     */
    suspend fun selectedText(): String? = withContext(Dispatchers.Default) {
        // The document may be closed meanwhile.
        runCatching { session.selectedText() }.getOrNull()
    }

    /**
     * What to draw.
     *
     * Changes whenever the view or the available tiles change, that is on every step of a scroll:
     * only the page canvas should read it. The bars around it read [currentPage] and [thumbnailId],
     * which change far less often.
     */
    var frame: ViewFrame? by mutableStateOf(null)
        private set

    /** Zero-based index of the page being looked at. With two pages, the first of the pair. */
    var currentPage: Int by mutableIntStateOf(0)
        private set

    /** The last page shown with [currentPage]: itself, or the second of the pair. */
    var lastPage: Int by mutableIntStateOf(0)
        private set

    /** Wide windows: whether the sidebar with Pages and Contents is open. */
    var sidebarOpen: Boolean by mutableStateOf(sidebarOpen)

    /** The frame that last changed which thumbnails are available. */
    private var thumbnails: ViewFrame? by mutableStateOf(null)

    /**
     * The tile to show as thumbnail of [page] (zero-based), or 0 if it isn't rendered yet or wasn't
     * announced with [showThumbnails].
     */
    fun thumbnailId(page: Int): Long = thumbnails?.thumbnailId(page) ?: 0

    /**
     * The frame whose [ViewFrame.thumbnailShapes] to draw over the thumbnails, or null before the
     * first one.
     */
    val thumbnailFrame: ViewFrame? get() = thumbnails

    fun bitmap(id: Long): Bitmap? = bitmaps[id]

    fun setViewport(widthDp: Float, heightDp: Float, density: Float) {
        // Through JNI: the bars around the view are animated, which resizes it on every frame.
        Native.setViewport(sessionId, widthDp.toDouble(), heightDp.toDouble(), density.toDouble())
        viewport = Offset(widthDp, heightDp)
        this.density = density
        // A position only means something once the view has a size.
        pendingRestore?.let { session.restoreView(it.copy(twoPages = it.twoPages && wideWindow)) }
        pendingRestore = null
        refresh()
    }

    /** Where the reader is now, for [DocumentStore]. */
    fun savedView(): SavedView =
        // What the reader chose, even if this window is too narrow to show it.
        (pendingRestore ?: session.savedView()).copy(twoPages = twoPages, coverAlone = coverAlone)

    fun scrollBy(delta: Offset) {
        Native.scrollBy(sessionId, delta.x.toDouble(), delta.y.toDouble())
        refresh()
    }

    fun zoomBy(factor: Float, anchor: Offset) {
        Native.zoomBy(sessionId, factor.toDouble(), anchor.x.toDouble(), anchor.y.toDouble())
        refresh()
    }

    /**
     * A tap at [at]: follows a link there, if any.
     *
     * Links within the document are followed right away ([Activated.Moved]). For an external one
     * ([Activated.Uri]), opening it after asking is up to the caller. [Activated.Nothing] means the
     * tap hit no link.
     */
    fun tap(at: Offset): Activated = finish(session.tap(at.x.toDouble(), at.y.toDouble()))

    /** Goes where entry [index] of [outline] leads (see [tap] for the result). */
    fun openOutlineEntry(index: Int): Activated = finish(session.activateOutline(index.toUInt()))

    private fun finish(activated: Activated): Activated {
        if (activated == Activated.Moved) {
            stopMotion()
            refresh()
        }
        return activated
    }

    // Keyboard

    /** The next or previous page. With two pages, the next or previous pair. */
    fun turnPage(forward: Boolean) {
        stopMotion()
        session.turnPage(forward)
        refresh()
    }

    /**
     * Page Down and Page Up: most of a screenful.
     *
     * When [paged], that is a page turn, unless the page is taller than the view (fitted to the
     * width, or zoomed in) and has more to show in that direction.
     */
    fun pageBy(scope: CoroutineScope, forward: Boolean) {
        val current = frame ?: return
        val distance = viewport.y * SCREENFUL * (if (forward) 1 else -1)
        if (current.paged) {
            if (!hasMore(current, forward)) return turnPage(forward)
            stopMotion()
            motion = scope.launch {
                var done = 0f
                animate(0f, 1f, animationSpec = tween(SLIDE_MS, easing = LinearOutSlowInEasing)) {
                        t,
                        _,
                    ->
                    Native.scrollInPageBy(sessionId, 0.0, (distance * (t - done)).toDouble())
                    done = t
                    refresh()
                }
            }
            return
        }
        stopMotion()
        motion = scope.launch { slideBy(Offset(0f, distance)) }
    }

    /**
     * Arrow up and down: a small scroll, or, where a page has no more to show in that direction, a
     * page turn.
     */
    fun lineBy(forward: Boolean) {
        val current = frame ?: return
        if (current.paged && !hasMore(current, forward)) return turnPage(forward)
        stopMotion()
        Native.scrollInPageBy(sessionId, 0.0, (if (forward) LINE_DP else -LINE_DP).toDouble())
        refresh()
    }

    /** Whether the pages being looked at continue beyond the view's bottom (or top) edge. */
    private fun hasMore(f: ViewFrame, forward: Boolean): Boolean =
        if (forward) f.currentBottom() > viewport.y * density + 1 else f.currentTop() < -1

    /** Zooms around the middle of the view. */
    fun zoomStep(zoomIn: Boolean) {
        stopMotion()
        zoomBy(if (zoomIn) ZOOM_STEP else 1 / ZOOM_STEP, viewport / 2f)
        // Not part of a gesture: let the view sharpen.
        endZoomGesture()
    }

    /** Back to the fitted view. */
    fun fit() {
        stopMotion()
        session.fit()
        refresh()
    }

    /** Shows [page] (zero-based) at the top of the view. */
    fun goToPage(page: Int) {
        stopMotion()
        session.goToPage(page.coerceIn(0, pageCount - 1).toUInt())
        refresh()
    }

    /**
     * Tells which pages' thumbnails are on screen, so that they get rendered and kept: see
     * [ViewFrame.thumbnailId].
     *
     * One range at a time: [owner] identifies the caller (the page grid, the thumbnail strip), and
     * the last one to call wins.
     */
    fun showThumbnails(owner: Any, first: Int, count: Int) {
        if (closed) return
        thumbnailOwner = owner
        Native.setThumbnailRange(sessionId, first, count)
        refresh()
    }

    /** [owner] no longer shows thumbnails. Does nothing if another took over since. */
    fun hideThumbnails(owner: Any) {
        if (closed || thumbnailOwner !== owner) return
        thumbnailOwner = null
        Native.setThumbnailRange(sessionId, 0, 0)
        refresh()
    }

    /**
     * Two pages side by side, as the reader asked for.
     *
     * Only takes effect in a [wideWindow]. On a narrow one the wish is kept (and saved) but pages
     * are shown one at a time.
     */
    var twoPages: Boolean by mutableStateOf(restore?.twoPages ?: false)
        private set

    /** With two pages: the first page on its own, like a book's cover. */
    var coverAlone: Boolean by mutableStateOf(restore?.coverAlone ?: false)
        private set

    /** Whether the window is wide enough for two pages. Follows rotation and split screen. */
    var wideWindow: Boolean = wideWindow
        set(value) {
            if (field == value) return
            field = value
            applySpread()
        }

    fun setSpread(twoPages: Boolean, coverAlone: Boolean) {
        this.twoPages = twoPages
        this.coverAlone = coverAlone
        applySpread()
    }

    private fun applySpread() {
        if (closed) return
        stopMotion()
        session.setSpread(twoPages && wideWindow, coverAlone)
        refresh()
    }

    /** A pinch step: zoom and pan together, planning only one frame. */
    fun transformBy(zoom: Float, anchor: Offset, pan: Offset) {
        if (zoom != 1f) {
            Native.zoomBy(
                sessionId,
                zoom.toDouble(),
                anchor.x.toDouble(),
                anchor.y.toDouble(),
            )
        }
        if (pan != Offset.Zero) Native.scrollBy(sessionId, pan.x.toDouble(), pan.y.toDouble())
        refresh()
    }

    /**
     * Brackets a pinch.
     *
     * In between, pages are drawn from a few fixed zoom levels. Exact-scale tiles are only rendered
     * once the zoom has come to rest.
     */
    fun beginZoomGesture() {
        session.beginZoomGesture()
    }

    fun endZoomGesture() {
        session.endZoomGesture()
        refresh()
    }

    /** A finger went down: whatever was animating stops where it is. */
    fun stopMotion() {
        motion?.cancel()
        motion = null
    }

    /**
     * The fingers lifted after a drag or pinch.
     *
     * [velocityPx] is the scrolling velocity at that moment in pixels per second. In paged mode a
     * view left between two pages slides on to one of them (a flick turns the page). Otherwise the
     * scrolling carries on and slows down, within the page when paged.
     */
    fun release(
        scope: CoroutineScope,
        velocityPx: Offset,
        density: Float,
        decay: DecayAnimationSpec<Offset>,
    ) {
        stopMotion()
        val current = frame ?: return
        if (current.paged) {
            val snap = session.snapDelta(
                (velocityPx.x / density).toDouble(),
                (velocityPx.y / density).toDouble(),
            )
            if (snap != null) {
                motion = scope.launch { slideBy(Offset(snap.x.toFloat(), snap.y.toFloat())) }
                return
            }
            // A page that is all there has nowhere to go.
            if (current.currentFitsIn(viewport.x * density + 1, viewport.y * density + 1)) return
        }
        motion = scope.launch {
            var last = Offset.Zero
            AnimationState(Offset.VectorConverter, Offset.Zero, velocityPx).animateDecay(decay) {
                val step = (value - last) / density
                last = value
                Native.scrollInPageBy(sessionId, step.x.toDouble(), step.y.toDouble())
                refresh()
            }
        }
    }

    /** Slides the view by [delta] dp and makes sure it ends at rest. */
    private suspend fun slideBy(delta: Offset) {
        var done = 0f
        animate(0f, 1f, animationSpec = tween(SLIDE_MS, easing = LinearOutSlowInEasing)) { t, _ ->
            scrollBy(delta * (t - done))
            done = t
        }
        // Rounding along the way may leave a fraction of a pixel.
        session.snapDelta(0.0, 0.0)?.let { scrollBy(Offset(it.x.toFloat(), it.y.toFloat())) }
    }

    /** Zooms in around [anchor], or back out to the fitted view if already zoomed in. */
    fun doubleTap(scope: CoroutineScope, anchor: Offset) {
        val current = frame ?: return
        val zoomIn = current.zoomFactor < 1.05
        val ratio = (if (zoomIn) DOUBLE_TAP_ZOOM else 1.0) / current.zoomFactor
        stopMotion()
        motion = scope.launch {
            beginZoomGesture()
            try {
                var applied = 1.0
                animate(0f, 1f, animationSpec = tween(220)) { t, _ ->
                    val target = ratio.pow(t.toDouble())
                    zoomBy((target / applied).toFloat(), anchor)
                    applied = target
                }
                // Back to following the window size (on rotation, for instance).
                if (!zoomIn) session.fit()
            } finally {
                // Also when a touch interrupts the animation.
                if (!closed) endZoomGesture()
            }
        }
    }

    /** Drops every tile that isn't on screen (`onTrimMemory`). */
    fun trimMemory() {
        if (closed) return
        session.trimMemory()
        refresh()
        Log.i(TAG, "trimmed to ${bitmaps.size()} tiles")
    }

    private fun collectTiles() {
        if (closed) return
        // Tiles that don't fit this time are rendered again. They are 16 bytes each, so the buffer
        // holds about a thousand.
        if (Native.takeFinished(sessionId, buffer) > 0) {
            val b = buffer
            b.position(0)
            val count = b.getInt()
            b.getInt()
            repeat(count) {
                val id = b.getLong()
                val bitmap = Bitmap.createBitmap(b.getInt(), b.getInt(), Bitmap.Config.ARGB_8888)
                if (Native.copyTile(sessionId, id, bitmap) == 0) {
                    bitmap.prepareToDraw()
                    bitmaps.put(id, bitmap)
                } else {
                    bitmap.recycle()
                }
            }
        }
        refresh()
    }

    private fun refresh() {
        if (closed) return
        var length = Native.frame(sessionId, buffer)
        if (length < 0) {
            buffer = newBuffer(-length * 2)
            length = Native.lastFrame(sessionId, buffer)
        }
        if (length <= 0) return
        val next = ViewFrame.decode(buffer, length)
        // Free the pixels now rather than whenever the GC gets to them. The new frame never draws
        // an evicted tile, and a frame already handed to the render thread keeps its own reference
        // to the pixels.
        for (id in next.evicted) {
            bitmaps[id]?.recycle()
            bitmaps.remove(id)
        }
        frame = next
        pagedNow = next.paged
        hasSelection = next.selection
        currentPage = next.currentPage
        lastPage = next.lastPage
        if (!next.sameThumbnailsAs(thumbnails)) thumbnails = next
        // After zooming, the view sharpens (switches to exact-scale tiles) only when a frame is
        // planned once the zoom has been still for a while.
        main.removeCallbacks(settle)
        if (next.settlesInMs >= 0) main.postDelayed(settle, next.settlesInMs + 1L)
    }

    override fun close() {
        stopMotion()
        closed = true
        main.removeCallbacks(settle)
        thumbnails = null
        frame = null
        for (i in 0 until bitmaps.size()) bitmaps.valueAt(i).recycle()
        bitmaps.clear()
        // Also releases the listener, which the session holds on to.
        session.close()
    }
}
