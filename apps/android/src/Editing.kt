package net.xfbs.specto

import android.content.Context
import android.net.Uri
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.io.File
import java.io.RandomAccessFile
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.specto_ffi.EditKind
import uniffi.specto_ffi.EditStep

/** The menu item that takes [step] back, such as "Undo rotate" or "Undo delete". */
@StringRes
internal fun undoLabel(step: EditStep): Int = when (step.kind) {
    EditKind.ROTATE -> R.string.undo_rotate
    EditKind.DELETE -> R.string.undo_delete
    EditKind.MOVE -> R.string.undo_move
    EditKind.INSERT -> R.string.undo_insert
    EditKind.ADD_ANNOTATION -> R.string.undo_add_annotation
    EditKind.REMOVE_ANNOTATION -> R.string.undo_remove_annotation
    EditKind.CHANGE_ANNOTATION -> R.string.undo_change_annotation
    EditKind.FILL_FIELD -> R.string.undo_fill_field
    EditKind.RESET_FORM -> R.string.undo_reset_form
}

/** The menu item that does [step] again, such as "Redo rotate". */
@StringRes
internal fun redoLabel(step: EditStep): Int = when (step.kind) {
    EditKind.ROTATE -> R.string.redo_rotate
    EditKind.DELETE -> R.string.redo_delete
    EditKind.MOVE -> R.string.redo_move
    EditKind.INSERT -> R.string.redo_insert
    EditKind.ADD_ANNOTATION -> R.string.redo_add_annotation
    EditKind.REMOVE_ANNOTATION -> R.string.redo_remove_annotation
    EditKind.CHANGE_ANNOTATION -> R.string.redo_change_annotation
    EditKind.FILL_FIELD -> R.string.redo_fill_field
    EditKind.RESET_FORM -> R.string.redo_reset_form
}

/**
 * The top bar while pages are picked in the page grid: how many, and what can be done with them.
 * Its ✕ (like Back) lets go of them.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SelectionBar(state: PdfViewState) {
    val picked = state.selectedPages
    TopAppBar(
        windowInsets = WindowInsets.safeDrawing.only(
            WindowInsetsSides.Horizontal + WindowInsetsSides.Top,
        ),
        navigationIcon = {
            IconButton(onClick = { state.selectedPages = emptySet() }) {
                Icon(
                    Icons.Filled.Close,
                    contentDescription = stringResource(R.string.clear_selection),
                )
            }
        },
        title = {
            // "3 selected": short, to fit a phone's top bar next to its buttons.
            Text(
                pluralStringResource(R.plurals.pages_selected, picked.size, picked.size),
                maxLines = 1,
            )
        },
        actions = {
            IconButton(onClick = { state.rotatePages(picked, clockwise = false) }) {
                Icon(rotateLeftIcon, contentDescription = stringResource(R.string.rotate_left))
            }
            IconButton(onClick = { state.rotatePages(picked, clockwise = true) }) {
                Icon(rotateRightIcon, contentDescription = stringResource(R.string.rotate_right))
            }
            // A document keeps at least one page.
            IconButton(onClick = {
                state.deletePages(picked)
            }, enabled = picked.size < state.pageCount) {
                Icon(
                    Icons.Filled.Delete,
                    contentDescription = stringResource(R.string.delete_pages),
                )
            }
            var menu by remember { mutableStateOf(false) }
            IconButton(onClick = { menu = true }) {
                Icon(Icons.Filled.MoreVert, contentDescription = stringResource(R.string.more))
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.select_all)) },
                    onClick = {
                        menu = false
                        state.selectedPages = (0 until state.pageCount).toSet()
                    },
                )
            }
        },
    )
}

/**
 * Asked before a document with unsaved edits is left: nothing is saved unless the reader says
 * where.
 */
@Composable
fun UnsavedChangesDialog(
    name: String,
    /**
     * Whether saving means asking where ("Save as"), since the document's own file can't be written
     * over.
     */
    saveAs: Boolean,
    onSave: () -> Unit,
    onDiscard: () -> Unit,
    onCancel: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.unsaved_title)) },
        text = { Text(stringResource(R.string.unsaved_text, name)) },
        confirmButton = {
            TextButton(onClick = onSave) {
                Text(stringResource(if (saveAs) R.string.save_as else R.string.save))
            }
        },
        dismissButton = {
            TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
            TextButton(onClick = onDiscard) { Text(stringResource(R.string.discard)) }
        },
    )
}

/**
 * Asks whether to let go of the unsaved changes to the document [name], which can't be undone.
 *
 * [onDiscard] lets go of them, [onCancel] keeps them.
 */
@Composable
fun DiscardChangesDialog(name: String, onDiscard: () -> Unit, onCancel: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.discard_title)) },
        text = { Text(stringResource(R.string.discard_text, name)) },
        confirmButton = {
            TextButton(onClick = onDiscard) { Text(stringResource(R.string.discard)) }
        },
        dismissButton = {
            TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/**
 * Says [what] the app is busy with (writing the document, say). Nothing else can be done meanwhile.
 */
@Composable
fun BusyDialog(what: String) {
    AlertDialog(
        onDismissRequest = {},
        confirmButton = {},
        text = { Text(what) },
    )
}

/** Where a document being saved is written first (see [writeEdited]). */
private const val SAVING_DIR = "saving"

/**
 * Writes the document open in [state], as edited, to a file of the app's own and returns it.
 *
 * From there it is copied to where the reader wants it once nothing reads the original any more: a
 * large original is mapped while open, and a mapped file must not change, which it would if the
 * reader chose to save over it. Throws if it can't be written.
 */
suspend fun Context.writeEdited(state: PdfViewState, name: String): File {
    val file = withContext(Dispatchers.IO) {
        val dir = File(cacheDir, SAVING_DIR)
        dir.deleteRecursively()
        dir.mkdirs()
        // Under the document's name: should copying it fail, it is what the reader is left with.
        File(dir, sharedFileName(name))
    }
    state.writeTo(file.writableDescriptor(), countsAsSaved = false)
    return file
}

/** Where a document written by [writeEdited] is kept when it couldn't be copied where it belongs. */
private const val UNSAVED_DIR = "unsaved"

/**
 * Moves [file], written by [writeEdited] and not copied where it belongs, to where it can stay open
 * and be saved again. Returns where it is then.
 *
 * Opened from where [writeEdited] writes, the next save would write the document over itself. If
 * it can't be moved, it stays.
 */
suspend fun Context.keepUnsaved(file: File): File = withContext(Dispatchers.IO) {
    val dir = File(cacheDir, UNSAVED_DIR)
    dir.deleteRecursively()
    dir.mkdirs()
    val kept = File(dir, file.name)
    if (file.renameTo(kept)) kept else file
}

/** Copies [file] to [destination], replacing what is there. Throws if that fails. */
suspend fun Context.copyTo(file: File, destination: Uri) = withContext(Dispatchers.IO) {
    checkNotNull(contentResolver.openOutputStream(destination, "wt")) {
        "cannot write to $destination"
    }.use { out ->
        file.inputStream().use { it.copyTo(out) }
    }
}

/** What [DocumentStore] recognizes this file by. */
internal fun File.fingerprint(): String = RandomAccessFile(this, "r").use { file ->
    DocumentStore.fingerprint(file.length()) { offset, length ->
        ByteArray(length).also {
            file.seek(offset)
            file.readFully(it)
        }
    }
}

/** An open descriptor to write this file from its start, for Rust to take over and close. */
internal fun File.writableDescriptor(): Int {
    val mode = android.os.ParcelFileDescriptor.MODE_WRITE_ONLY or
        android.os.ParcelFileDescriptor.MODE_CREATE or
        android.os.ParcelFileDescriptor.MODE_TRUNCATE
    return android.os.ParcelFileDescriptor.open(this, mode).detachFd()
}

// Icons

/** Three quarters of a circle with an arrowhead where it ends, at the top. */
private fun rotateIcon(name: String, clockwise: Boolean): ImageVector {
    // Drawn for clockwise and mirrored for the other way.
    fun x(x: Float) = if (clockwise) x else 24f - x
    return ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).apply {
        path(
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
        ) {
            moveTo(x(19f), 13f)
            arcTo(7f, 7f, 0f, isMoreThanHalf = true, isPositiveArc = clockwise, x(12f), 6f)
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(x(12f), 2f)
            lineTo(x(17f), 6f)
            lineTo(x(12f), 10f)
            close()
        }
    }.build()
}

private val rotateRightIcon = rotateIcon("RotateRight", clockwise = true)
private val rotateLeftIcon = rotateIcon("RotateLeft", clockwise = false)
