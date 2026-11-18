package net.xfbs.specto

import kotlin.math.hypot

/**
 * A touch that took hold of one of the selection's handles: which one, and how far the middle of
 * the line's edge is from the touch.
 *
 * Adding that to the finger's position as it moves gives the point in the text that the end of the
 * selection should follow, rather than the point under the finger, which is below the line.
 */
class HandleGrab(val end: Boolean, val dx: Float, val dy: Float)

/**
 * Where the round part of a handle is: hanging below the line, outside the selection, with its
 * corner on the bottom of [edge] (bottom x, bottom y, top x, top y).
 *
 * The handle at the selection's start hangs to the left, the one at its [end] to the right.
 */
fun handleCenter(edge: FloatArray, end: Boolean, radius: Float): Pair<Float, Float> =
    (edge[0] + if (end) radius else -radius) to (edge[1] + radius)

/**
 * The handle a touch at ([x], [y]) takes hold of: the nearest one whose center is within [reach],
 * if any.
 *
 * Handles are small, so [reach] is more generous than what is drawn.
 */
fun ViewFrame.handleAt(x: Float, y: Float, radius: Float, reach: Float): HandleGrab? {
    var grab: HandleGrab? = null
    var nearest = reach
    for (end in listOf(false, true)) {
        val edge = (if (end) selectionEnd else selectionStart) ?: continue
        val (cx, cy) = handleCenter(edge, end, radius)
        val distance = hypot(x - cx, y - cy)
        if (distance <= nearest) {
            nearest = distance
            grab = HandleGrab(end, (edge[0] + edge[2]) / 2 - x, (edge[1] + edge[3]) / 2 - y)
        }
    }
    return grab
}
