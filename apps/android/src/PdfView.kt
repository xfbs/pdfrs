package net.xfbs.specto

import android.content.ClipData
import android.content.ClipboardManager
import android.graphics.BlendMode
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.RectF
import android.os.Build
import android.util.Log
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.splineBasedDecay
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.specto_ffi.Activated
import uniffi.specto_ffi.Markup
import uniffi.specto_ffi.PdfException

/** Radius of the round part of a selection handle. */
private val HANDLE_RADIUS = 10.dp

/** How far from a handle's center a touch still takes hold of it. */
private val HANDLE_REACH = 28.dp

/**
 * While scrolling continuously, a selection dragged this close to the top or bottom scrolls the
 * view.
 */
private val AUTO_SCROLL_ZONE = 56.dp

/** How fast that goes at most, in dp per second. */
private const val AUTO_SCROLL_SPEED = 1400f

/** How long the view has to hold still before the copy menu (re)appears. */
private const val MENU_DELAY_MS = 150L

/** Beyond this many characters the system can't take a text onto the clipboard. */
private const val CLIPBOARD_MAX_CHARS = 300_000

/**
 * The most text offered to other apps at once.
 *
 * Less than fits on the clipboard: a shared text travels inside the request to the share sheet and
 * again inside the one to the app chosen, and those have a size limit that everything else on its
 * way at that moment counts towards.
 */
internal const val SHARE_MAX_CHARS = 100_000

/** How thick the box around a picked annotation is drawn. */
private val PICKED_OUTLINE = 2.dp

private val MATCH_COLOR = Color(0xFFFFD600)
private val CURRENT_MATCH_COLOR = Color(0xFFFF6D00)

/** Where a finger is dragging the selection, between pointer events. */
private class SelectionDrag {
    /** A long press selected a word, and the finger is still down. */
    var byPress = false

    /**
     * The point in the view (pixels) the moving end of the selection follows, or null if none is.
     */
    var at: Offset? = null
}

/**
 * What keys do while the page has the focus, as on the desktop.
 *
 * - Arrows left and right, Page Up and Down, Space and Backspace turn pages.
 * - Arrows up and down scroll.
 * - Home and End go to the first and last page.
 * - Ctrl with plus, minus and zero zooms.
 * - Ctrl+A selects all text, Ctrl+C copies the selection ([onCopy]) and Escape drops it.
 * - Ctrl+F searches ([onSearch]), Ctrl+S saves ([onSave]) and Ctrl+P prints ([onPrint]).
 * - Ctrl+Z takes back the last edit of the pages, and Shift+Ctrl+Z or Ctrl+Y does it again.
 *
 * Returns whether the key was used.
 */
private fun PdfViewState.handleKey(
    scope: CoroutineScope,
    event: KeyEvent,
    onCopy: () -> Unit,
    onSearch: () -> Unit,
    onSave: () -> Unit,
    onPrint: () -> Unit,
): Boolean {
    if (event.isCtrlPressed) {
        when (event.key) {
            Key.Equals, Key.Plus, Key.NumPadAdd -> zoomStep(zoomIn = true)
            Key.Minus, Key.NumPadSubtract -> zoomStep(zoomIn = false)
            Key.Zero, Key.NumPad0 -> fit()
            Key.A -> selectAll()
            Key.C -> if (hasSelection) onCopy() else return false
            Key.F -> onSearch()
            Key.S -> onSave()
            Key.P -> onPrint()
            Key.Z -> if (event.isShiftPressed) redo() else undo()
            Key.Y -> redo()
            else -> return false
        }
        return true
    }
    if (event.isAltPressed || event.isMetaPressed) return false
    when (event.key) {
        Key.Escape -> if (hasSelection) clearSelection() else return false
        Key.DirectionRight -> turnPage(forward = true)
        Key.DirectionLeft, Key.Backspace -> turnPage(forward = false)
        Key.PageDown -> pageBy(scope, forward = true)
        Key.PageUp -> pageBy(scope, forward = false)
        Key.Spacebar -> pageBy(scope, forward = !event.isShiftPressed)
        Key.DirectionDown -> lineBy(forward = true)
        Key.DirectionUp -> lineBy(forward = false)
        Key.MoveHome -> goToPage(0)
        Key.MoveEnd -> goToPage(pageCount - 1)
        else -> return false
    }
    return true
}

/**
 * Draws the frames planned by [state] and turns touches into scrolling or swiping between pages,
 * pinch zoom and double-tap zoom.
 *
 * A tap follows a link. Links leaving the document are passed to [onExternalLink], and taps that
 * hit no link to [onTap]. With a keyboard attached, keys turn pages, scroll and zoom. No layout
 * math happens here.
 *
 * A long press selects the word under the finger, and moving on before lifting it selects further
 * words. The selection then has a handle at each end to drag, and the system's text menu with
 * "Copy", "Share" and "Select all". A tap or Back drops it.
 *
 * What a search found is highlighted too. The search itself is run from outside (see
 * [PdfViewState.search]). [onSearch] is called for Ctrl+F, and [onSave] for Ctrl+S and [onPrint]
 * for Ctrl+P. The page takes the keyboard focus when it appears, and again whenever [refocus]
 * changes.
 *
 * A page that takes long to draw has a spinner on it until it is sharp (see [BusyPages]).
 */
@Composable
fun PdfView(
    state: PdfViewState,
    modifier: Modifier = Modifier,
    onExternalLink: (String) -> Unit = {},
    onTap: () -> Unit = {},
    onSearch: () -> Unit = {},
    onSave: () -> Unit = {},
    onPrint: () -> Unit = {},
    refocus: Any? = null,
) {
    Box(modifier) {
        PageCanvas(
            state,
            Modifier.fillMaxSize(),
            onExternalLink,
            onTap,
            onSearch,
            onSave,
            onPrint,
            refocus,
        )
        BusyPages(state)
        FieldDialog(state)
    }
}

/** The pages themselves, and everything touches and keys do: see [PdfView]. */
@Composable
private fun PageCanvas(
    state: PdfViewState,
    modifier: Modifier,
    onExternalLink: (String) -> Unit,
    onTap: () -> Unit,
    onSearch: () -> Unit,
    onSave: () -> Unit,
    onPrint: () -> Unit,
    refocus: Any?,
) {
    val density = LocalDensity.current.density
    val textSelected = stringResource(R.string.text_selected)
    val scope = rememberCoroutineScope()
    val exactPaint = remember { Paint().apply { isFilterBitmap = false } }
    val scaledPaint = remember { Paint().apply { isFilterBitmap = true } }
    val dest = remember { RectF() }
    val shapes = remember { ShapePainter() }
    val annotationSelected = stringResource(R.string.annotation_selected)
    var origin by remember { mutableStateOf(Offset.Zero) }

    // Text selection
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    val view = LocalView.current
    val menu = remember(view) { TextMenu(view) }
    // Pages are white whatever the theme: take the darker of the two accents.
    val accent = MaterialTheme.colorScheme.run {
        if (primary.luminance() > 0.5f) inversePrimary else primary
    }
    val handleRadius = HANDLE_RADIUS.value * density
    // One path per kind of highlight, in the order of `ViewFrame.HIGHLIGHT_*`.
    val highlights = remember { List(3) { Path() } }
    val drag = remember(state) { SelectionDrag() }
    // A finger is moving an end of the selection (a handle, or after the long press).
    var selecting by remember(state) { mutableStateOf(false) }
    var viewSize by remember { mutableStateOf(IntSize.Zero) }
    var originInRoot by remember { mutableStateOf(Offset.Zero) }

    fun copy() {
        scope.launch {
            val text = state.selectedText()
            // As everywhere on Android, copying ends the selection.
            state.clearSelection()
            if (text == null) return@launch
            val copied = text.length <= CLIPBOARD_MAX_CHARS && try {
                context.getSystemService(ClipboardManager::class.java)
                    .setPrimaryClip(
                        ClipData.newPlainText(context.getString(R.string.clip_label), text),
                    )
                true
            } catch (e: RuntimeException) {
                // Still too large for the system to pass around.
                false
            }
            // Android 13 and later show what was copied themselves.
            if (!copied) {
                Toast.makeText(context, R.string.too_much_to_copy, Toast.LENGTH_LONG).show()
            } else if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                Toast.makeText(context, R.string.copied, Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun share() {
        scope.launch {
            val text = state.selectedText()
            // As in a text field, sharing ends the selection.
            state.clearSelection()
            if (text.isNullOrEmpty()) return@launch
            val shared = text.length <= SHARE_MAX_CHARS && try {
                context.shareText(text)
                true
            } catch (e: RuntimeException) {
                // Still too large, or nothing on this device takes text.
                false
            }
            if (!shared) {
                Toast.makeText(context, R.string.too_much_to_share, Toast.LENGTH_LONG).show()
            }
        }
    }

    fun mark(markup: Markup) {
        state.markSelection(markup)
    }

    fun deleteAnnotation() {
        try {
            state.deletePickedAnnotation()
        } catch (e: PdfException.Save) {
            // One of the file's own, on a page that can't be written anew.
            Toast.makeText(
                context,
                context.getString(R.string.could_not_delete_annotation, e.reason),
                Toast.LENGTH_LONG,
            ).show()
        }
    }

    // Back first lets go of the selection, or of the annotation that is picked.
    BackHandler(enabled = state.hasSelection) { state.clearSelection() }
    BackHandler(enabled = state.hasPickedAnnotation) { state.clearPickedAnnotation() }

    // While scrolling continuously, a selection dragged to the top or bottom edge scrolls the view
    // on, so it can grow beyond what is on screen. How hard the finger pushes: -1 at the top edge,
    // 1 at the bottom one, 0 away from both.
    fun edgePush(at: Offset): Float {
        val zone = AUTO_SCROLL_ZONE.value * density
        return when {
            state.paged -> 0f
            at.y < zone -> (at.y - zone) / zone
            at.y > viewSize.height - zone -> (at.y - (viewSize.height - zone)) / zone
            else -> 0f
        }.coerceIn(-1f, 1f)
    }
    var atEdge by remember(state) { mutableStateOf(false) }

    /** The finger moved the selection's end to [at] (pixels), or let go of it (null). */
    fun follow(at: Offset?) {
        drag.at = at
        atEdge = at != null && edgePush(at) != 0f
        if (at != null) state.selectTo(at / density)
    }
    LaunchedEffect(state, atEdge) {
        if (!atEdge) return@LaunchedEffect
        var last = withFrameNanos { it }
        while (true) {
            val now = withFrameNanos { it }
            val seconds = (now - last) / 1e9f
            last = now
            val at = drag.at ?: break
            state.scrollBy(Offset(0f, edgePush(at) * AUTO_SCROLL_SPEED * seconds))
            state.selectTo(at / density)
        }
    }

    // The floating text menu, beside the selected text that is on screen. It goes away while the
    // view or the selection is moving and comes back once they rest.
    val selected = if (state.hasSelection) {
        state.frame?.selectionBounds()?.let { Rect(it[0], it[1], it[2], it[3]) }
    } else {
        null
    }
    // An annotation picked by a tap gets the same menu, to delete it.
    val picked = if (state.hasPickedAnnotation) {
        state.frame?.pickedBounds()?.let { Rect(it[0], it[1], it[2], it[3]) }
    } else {
        null
    }
    LaunchedEffect(state, selected, picked, selecting, originInRoot) {
        menu.hide()
        if (selecting) return@LaunchedEffect
        val beside = (selected ?: picked ?: return@LaunchedEffect).translate(originInRoot)
        delay(MENU_DELAY_MS)
        val items = if (selected != null) {
            listOf(
                TextMenu.Item(context.getString(R.string.highlight)) { mark(Markup.HIGHLIGHT) },
                TextMenu.Item(context.getString(R.string.copy), ::copy),
                TextMenu.Item(context.getString(R.string.share), ::share),
                TextMenu.Item(context.getString(R.string.underline)) { mark(Markup.UNDERLINE) },
                TextMenu.Item(context.getString(R.string.strike_through)) {
                    mark(Markup.STRIKE_OUT)
                },
                TextMenu.Item(context.getString(R.string.select_all)) { state.selectAll() },
            )
        } else {
            listOf(TextMenu.Item(context.getString(R.string.delete), ::deleteAnnotation))
        }
        menu.show(beside.left, beside.top, beside.right, beside.bottom, items)
    }
    DisposableEffect(state) {
        onDispose { menu.hide() }
    }

    // Sharpness probe, off unless `adb shell setprop log.tag.specto DEBUG`: a screenshot's page
    // area should then match `specto render file.pdf -p <page> -s <device scale>` pixel for pixel.
    val frame = state.frame
    if (frame != null && frame.settled && frame.complete && Log.isLoggable("specto", Log.DEBUG)) {
        LaunchedEffect(frame) {
            if (frame.pageCount == 0) return@LaunchedEffect
            Log.d(
                "specto",
                "settled: device scale ${frame.deviceScale}, " +
                    "canvas origin (${origin.x}, ${origin.y}) px, " +
                    "page ${frame.page(0) + 1} at (${frame.pageLeft(0)}, ${frame.pageTop(0)}) " +
                    "${frame.pageWidth(0)}x${frame.pageHeight(0)}",
            )
        }
    }

    // Keys go to whatever has the focus, so the page takes it when it appears.
    val focus = remember { FocusRequester() }
    LaunchedEffect(state, refocus) { focus.requestFocus() }

    Canvas(
        modifier
            .clipToBounds()
            .focusRequester(focus)
            .focusable()
            .onKeyEvent {
                it.type == KeyEventType.KeyDown && state.handleKey(
                    scope,
                    it,
                    ::copy,
                    onSearch,
                    onSave,
                    onPrint,
                )
            }
            .onGloballyPositioned {
                origin = it.positionInWindow()
                originInRoot = it.positionInRoot()
            }
            .onSizeChanged {
                viewSize = it
                state.setViewport(it.width / density, it.height / density, density)
            }
            // For screen readers, and for UI tests, which can't see the page.
            .semantics {
                if (state.hasSelection) {
                    stateDescription = textSelected
                } else if (state.hasPickedAnnotation) {
                    stateDescription = annotationSelected
                }
            }
            .pointerInput(state) {
                detectTapGestures(
                    onTap = {
                        if (state.hasSelection) {
                            state.clearSelection()
                        } else {
                            // A link first, as on the desktop, then a field of the form, which
                            // the tap fills in, then an annotation, which it picks. A tap elsewhere
                            // lets go of the picked one, and does nothing else.
                            val hadPicked = state.hasPickedAnnotation
                            when (val hit = state.tap(it / density)) {
                                Activated.Nothing ->
                                    if (
                                        !state.tapField(it / density) &&
                                        !state.pickAnnotation(it / density) &&
                                        !hadPicked
                                    ) {
                                        onTap()
                                    }

                                Activated.Moved -> state.clearPickedAnnotation()

                                is Activated.Uri -> onExternalLink(hit.uri)
                            }
                        }
                    },
                    onDoubleTap = { state.doubleTap(scope, it / density) },
                    onLongPress = {
                        if (state.selectWordAt(it / density)) {
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            // The finger is still down: see the gesture below.
                            drag.byPress = true
                            selecting = true
                        }
                    },
                )
            }
            .pointerInput(state) {
                val decay = splineBasedDecay<Offset>(this)
                val reach = HANDLE_REACH.toPx()
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    state.stopMotion()

                    // A touch on one of the selection's handles moves that end of the selection,
                    // and does nothing else.
                    val grab = if (state.hasSelection) {
                        state.frame?.handleAt(down.position.x, down.position.y, handleRadius, reach)
                    } else {
                        null
                    }
                    if (grab != null) {
                        down.consume()
                        state.grabSelection(grab.end)
                        selecting = true
                        try {
                            while (true) {
                                val event = awaitPointerEvent()
                                val finger = event.changes.firstOrNull { it.id == down.id } ?: break
                                event.changes.forEach { it.consume() }
                                if (!finger.pressed) break
                                follow(finger.position + Offset(grab.dx, grab.dy))
                            }
                        } finally {
                            follow(null)
                            selecting = false
                        }
                        return@awaitEachGesture
                    }

                    val velocity = VelocityTracker()
                    var moved = Offset.Zero
                    var zoomed = 1f
                    var dragging = false
                    var zooming = false
                    do {
                        val event = awaitPointerEvent()
                        if (drag.byPress) {
                            // A long press selected a word (see above). As the finger moves on, so
                            // does the selection.
                            val finger = event.changes.firstOrNull { it.id == down.id }
                            if (finger != null && finger.pressed && finger.positionChanged()) {
                                follow(finger.position)
                            }
                            event.changes.forEach { if (it.positionChanged()) it.consume() }
                            continue
                        }
                        val pan = event.calculatePan()
                        val zoom = event.calculateZoom()
                        if (!dragging) {
                            // Ignore jitter until the fingers clearly move, so taps stay taps.
                            moved += pan
                            zoomed *= zoom
                            dragging = moved.getDistance() > viewConfiguration.touchSlop ||
                                (event.changes.size > 1 && abs(1 - zoomed) > 0.03f)
                        }
                        if (dragging) {
                            if (zoom != 1f && !zooming) {
                                zooming = true
                                state.beginZoomGesture()
                            }
                            state.transformBy(
                                zoom,
                                event.calculateCentroid(useCurrent = false) / density,
                                -pan / density,
                            )
                            event.changes.forEach { if (it.positionChanged()) it.consume() }
                        }
                        // Only a single finger lifting off leads to a fling.
                        if (event.changes.size == 1) {
                            // Includes the samples batched between frames.
                            velocity.addPointerInputChange(event.changes[0])
                        } else {
                            velocity.resetTracking()
                        }
                    } while (currentEvent.changes.any { it.pressed })
                    if (drag.byPress) {
                        drag.byPress = false
                        follow(null)
                        selecting = false
                    }
                    if (zooming) state.endZoomGesture()
                    if (dragging) {
                        val v = velocity.calculateVelocity()
                        state.release(scope, Offset(-v.x, -v.y), density, decay)
                    }
                }
            },
    ) {
        val f = state.frame ?: return@Canvas
        for (p in 0 until f.pageCount) {
            val left = f.pageLeft(p)
            val top = f.pageTop(p)
            val size = Size(f.pageWidth(p), f.pageHeight(p))
            drawRect(Color.White, Offset(left, top), size)
            clipRect(left, top, left + size.width, top + size.height) {
                drawIntoCanvas { canvas ->
                    for (t in f.tiles(p)) {
                        val bitmap = state.bitmap(f.tileId(t)) ?: continue
                        // Destinations can be fractional, so no IntOffset-based drawImage.
                        dest.set(f.tileLeft(t), f.tileTop(t), f.tileRight(t), f.tileBottom(t))
                        val paint = if (f.tileIsExact(t)) exactPaint else scaledPaint
                        canvas.nativeCanvas.drawBitmap(bitmap, null, dest, paint)
                    }
                    dest.set(left, top, left + size.width, top + size.height)
                    shapes.draw(canvas.nativeCanvas, f.shapes, p, dest)
                }
            }
        }
        // The box around the annotation that is picked.
        f.picked?.let { box ->
            val outline = Path().apply {
                moveTo(box[0], box[1])
                for (corner in 1..3) lineTo(box[2 * corner], box[2 * corner + 1])
                close()
            }
            drawPath(outline, accent, style = Stroke(width = PICKED_OUTLINE.toPx()))
        }
        // What the search found and what is selected, and a handle at each end of the selection
        // that is on screen.
        if (f.highlightCount > 0) {
            highlights.forEach { it.rewind() }
            for (i in 0 until f.highlightCount) {
                val path = highlights[f.highlightKind(i)]
                path.moveTo(f.highlightX(i, 0), f.highlightY(i, 0))
                for (corner in 1..3) path.lineTo(f.highlightX(i, corner), f.highlightY(i, corner))
                path.close()
            }
            drawPath(highlights[ViewFrame.HIGHLIGHT_MATCH], MATCH_COLOR, alpha = 0.4f)
            drawPath(
                highlights[ViewFrame.HIGHLIGHT_CURRENT_MATCH],
                CURRENT_MATCH_COLOR,
                alpha = 0.55f,
            )
            drawPath(highlights[ViewFrame.HIGHLIGHT_SELECTION], accent, alpha = 0.3f)
        }
        f.selectionStart?.let { drawHandle(it, end = false, handleRadius, accent) }
        f.selectionEnd?.let { drawHandle(it, end = true, handleRadius, accent) }
    }
}

/**
 * Draws [FrameShapes], the annotations that aren't part of the pages as rendered, reusing its
 * paints and path from one time to the next.
 */
internal class ShapePainter {
    private val path = android.graphics.Path()
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }

    // The layer of highlights is multiplied with the page under it. Before Android 10 there is no
    // such blend that leaves the page alone where the layer is empty (PorterDuff's MULTIPLY
    // blackens it), so there the darker of the two colors is taken: the same on white paper and
    // black text, and close to it elsewhere.
    private val darken = Paint().apply {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            blendMode = BlendMode.MULTIPLY
        } else {
            xfermode = PorterDuffXfermode(PorterDuff.Mode.DARKEN)
        }
    }

    /**
     * Draws those of [shapes] that are on [page] onto [canvas], which is clipped to that page.
     *
     * [bounds] is where the page is on the canvas. Highlights first: they darken the page under
     * them, together, so that two that overlap don't darken it twice. Then the rest, on top of the
     * page.
     */
    fun draw(canvas: android.graphics.Canvas, shapes: FrameShapes, page: Int, bounds: RectF) {
        var any = false
        var darkening = false
        for (i in 0 until shapes.count) {
            if (shapes.page(i) != page) continue
            any = true
            if (shapes.darkens(i)) darkening = true
        }
        if (!any) return
        if (darkening) {
            val layer = canvas.saveLayer(bounds, darken)
            for (i in 0 until shapes.count) {
                if (shapes.page(i) == page && shapes.darkens(i)) draw(canvas, shapes, i)
            }
            canvas.restoreToCount(layer)
        }
        for (i in 0 until shapes.count) {
            if (shapes.page(i) == page && !shapes.darkens(i)) draw(canvas, shapes, i)
        }
    }

    private fun draw(canvas: android.graphics.Canvas, shapes: FrameShapes, i: Int) {
        path.rewind()
        path.moveTo(shapes.startX(i), shapes.startY(i))
        for (s in shapes.segments(i)) {
            if (shapes.segmentIsCurve(s)) {
                path.cubicTo(
                    shapes.segmentX(s, 0),
                    shapes.segmentY(s, 0),
                    shapes.segmentX(s, 1),
                    shapes.segmentY(s, 1),
                    shapes.segmentX(s, 2),
                    shapes.segmentY(s, 2),
                )
            } else {
                path.lineTo(shapes.segmentX(s, 0), shapes.segmentY(s, 0))
            }
        }
        if (shapes.isClosed(i)) path.close()
        if (shapes.isFilled(i)) {
            fill.color = shapes.fill(i)
            canvas.drawPath(path, fill)
        }
        if (shapes.isStroked(i)) {
            val round = shapes.isRound(i)
            stroke.color = shapes.stroke(i)
            stroke.strokeWidth = shapes.strokeWidth(i)
            stroke.strokeCap = if (round) Paint.Cap.ROUND else Paint.Cap.BUTT
            stroke.strokeJoin = if (round) Paint.Join.ROUND else Paint.Join.MITER
            canvas.drawPath(path, stroke)
        }
    }
}

/**
 * A selection handle as Android draws them: a drop hanging below the line, its pointed corner at
 * the bottom of [edge] (see [handleCenter]).
 */
private fun DrawScope.drawHandle(edge: FloatArray, end: Boolean, radius: Float, color: Color) {
    val (x, y) = handleCenter(edge, end, radius)
    drawCircle(color, radius, Offset(x, y))
    // The quarter towards the text is square, which makes the point.
    drawRect(color, Offset(if (end) x - radius else x, y - radius), Size(radius, radius))
}
