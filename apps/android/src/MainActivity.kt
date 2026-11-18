package net.xfbs.specto

import android.app.Activity
import android.app.ActivityManager
import android.content.ActivityNotFoundException
import android.content.ComponentCallbacks2
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.OpenableColumns
import android.util.Log
import android.view.KeyEvent
import android.view.WindowManager
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import java.io.File
import kotlin.concurrent.thread
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.specto_ffi.Activated
import uniffi.specto_ffi.DraftStore
import uniffi.specto_ffi.PdfException
import uniffi.specto_ffi.PdfFile

class MainActivity : ComponentActivity() {
    /** The document to show: from "Open with", or picked in the app. */
    private var document by mutableStateOf<Uri?>(null)

    /** A document another app asked for while one is open: shown once the open one can be left. */
    private var incoming by mutableStateOf<Uri?>(null)
    private var current: Loaded.Ready? = null
    private lateinit var store: DocumentStore
    private lateinit var drafts: DraftStore
    private lateinit var settings: AppSettings

    /**
     * Moves on or back through the document being read, for the volume keys, or null while none is
     * on screen.
     */
    private var pageKeys: ((forward: Boolean) -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())
    private val letScreenTurnOff =
        Runnable { window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        store = DocumentStore(File(filesDir, "documents.json"))
        drafts = DraftStore.open(File(filesDir, "store").path)
        thread { runCatching { drafts.prune(DRAFT_DAYS.toUInt()) } }
        settings = AppSettings(getSharedPreferences("settings", Context.MODE_PRIVATE))
        // After the system ended the app in the background: the document that was open.
        document = if (savedInstanceState == null) {
            intent.viewedDocument()
        } else {
            savedInstanceState.getString(OPEN_DOCUMENT)?.let(Uri::parse)
        }
        setContent {
            // Also when the switch itself is flipped.
            val keepScreenOn = settings.keepScreenOn
            LaunchedEffect(keepScreenOn) { stayAwake() }
            AppTheme {
                App(
                    document = document,
                    store = store,
                    drafts = drafts,
                    settings = settings,
                    crashLog = (application as SpectoApp).crashLog,
                    onDocument = { document = it },
                    incoming = incoming,
                    onIncoming = { incoming = null },
                    onReady = {
                        current = it
                        stayAwake()
                    },
                    onPageKeys = { pageKeys = it },
                )
            }
        }
    }

    /**
     * With [AppSettings.keepScreenOn] and a document open, keeps the screen on for a while from
     * now.
     *
     * Called whenever the reader does something. A phone left lying on the table still goes dark in
     * the end.
     */
    private fun stayAwake() {
        main.removeCallbacks(letScreenTurnOff)
        if (settings.keepScreenOn && current != null) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            main.postDelayed(letScreenTurnOff, KEEP_SCREEN_ON_MINUTES * 60_000L)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    override fun onUserInteraction() {
        super.onUserInteraction()
        stayAwake()
    }

    override fun onDestroy() {
        main.removeCallbacks(letScreenTurnOff)
        super.onDestroy()
    }

    /**
     * Whether [keyCode] is a volume key that is to move through the document, and which way: true
     * for volume down (onwards).
     */
    private fun pageKey(keyCode: Int): Boolean? = when {
        pageKeys == null || !settings.volumeKeysTurnPages -> null
        keyCode == KeyEvent.KEYCODE_VOLUME_DOWN -> true
        keyCode == KeyEvent.KEYCODE_VOLUME_UP -> false
        else -> null
    }

    // Only keys nothing else wanted arrive here: with a menu or dialog open, the volume keys change
    // the volume as usual.
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val forward = pageKey(keyCode) ?: return super.onKeyDown(keyCode, event)
        pageKeys?.invoke(forward)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean =
        // Its key down was used, so the system shouldn't see half a key press.
        pageKey(keyCode) != null || super.onKeyUp(keyCode, event)

    override fun onStop() {
        super.onStop()
        // The app may not come back: remember where the reader is, and what isn't saved yet.
        current?.let {
            store.saveView(it.key, it.state.savedView(), it.state.sidebarOpen)
            it.keepDraft(drafts)
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        // Only where the app can still read it then: from the start screen, or from the intent
        // the activity comes back with.
        val reachable = document?.takeIf {
            it.scheme == "file" || it.toString() in store.reopenable() || it == intent.data
        }
        outState.putString(OPEN_DOCUMENT, reachable?.toString())
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The activity isn't recreated when dark mode is switched, so the system bar icons have to
        // be told about the new background.
        enableEdgeToEdge()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.viewedDocument()?.let { incoming = it }
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        // In the background, or the system is short on memory: keep only what is on screen. Tiles
        // are re-rendered on demand.
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) current?.state?.trimMemory()
    }
}

private fun Intent.viewedDocument(): Uri? = data.takeIf { action == Intent.ACTION_VIEW }

/** Names the open document in the state the activity keeps when the system ends the app. */
private const val OPEN_DOCUMENT = "document"

/** How long a draft of unsaved edits is kept without the document being opened. */
private const val DRAFT_DAYS = 30

/**
 * Follows the system: dark or light, and on Android 12+ the colors derived from the wallpaper.
 * Pages themselves stay as the document defines them.
 */
@Composable
private fun AppTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)

        dark -> darkColorScheme()

        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = colors, content = content)
}

/** Keeps content that isn't the page itself clear of the camera cutout and the navigation bar. */
private val besideAndBelow = WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom

/** Identifies the page area to UI tests. */
const val PAGE_VIEW_TAG = "page-view"

/** Identifies the sidebar of wide windows to UI tests. */
const val SIDEBAR_TAG = "sidebar"

/** Windows at least this wide (in dp) show Pages and Contents beside the document. */
const val WIDE_WINDOW_DP = 840

private val SIDEBAR_WIDTH = 300.dp

/** Duration of the bars sliding in and out. */
private const val BARS_MS = 220

/** How long the page number stays after a page turn in full screen. */
private const val INDICATOR_MS = 1500L

/**
 * Budget for tile bitmaps: a share of what the system considers normal for an app here.
 *
 * A drawn tile also has a copy on the GPU, so the real cost is up to twice this.
 */
private fun Context.tileCacheBytes(): Long {
    val memoryClassMb = (getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager).memoryClass
    return (memoryClassMb / 4).coerceIn(32, 128) * 1024L * 1024L
}

private fun Context.displayName(uri: Uri): String {
    val fromProvider = runCatching {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }
    }.getOrNull()
    return fromProvider ?: uri.lastPathSegment ?: getString(R.string.unnamed_document)
}

/**
 * Where the reader wants to go from a document with unsaved edits: another one, or (null) the start
 * screen.
 */
private class Leaving(val to: Uri?)

private sealed interface Loaded {
    data object Loading : Loaded

    /**
     * [key] identifies the document in the [DocumentStore].
     *
     * A [mapped] file (a large one) is read where it lies for as long as it is open, and must not
     * change meanwhile.
     */
    class Ready(
        val state: PdfViewState,
        val name: String,
        key: String,
        bytes: Long,
        val mapped: Boolean,
    ) : Loaded {
        // Both change when the document is saved over its file.
        var key: String = key
        var bytes: Long by mutableStateOf(bytes)

        /**
         * Whether unsaved edits are kept as a draft when the document is closed: not once they are
         * discarded, or saved somewhere else.
         */
        var keepsDraft = true

        /**
         * Keeps the unsaved edits in [drafts] for when the document is opened again, or deletes
         * the draft if there are none to keep.
         */
        fun keepDraft(drafts: DraftStore) {
            try {
                if (keepsDraft) state.keepDraft(drafts, key) else drafts.delete(key)
            } catch (e: PdfException) {
                Log.w("specto", "the draft of $name can't be kept", e)
            }
        }

        /** Lets the unsaved edits go: the document is closed without a draft. */
        fun dropDraft(drafts: DraftStore) {
            keepsDraft = false
            keepDraft(drafts)
        }
    }

    /** Keeps the [file], so that trying a password doesn't read it again. */
    class Locked(val file: PdfFile, val name: String, val uri: Uri, val wrongPassword: Boolean) :
        Loaded
    class Failed(val message: String) : Loaded
}

/**
 * Makes sure the app can read [uri] again later, so that the document can be offered on the start
 * screen.
 *
 * Only some addresses allow that: those from the system file picker do, most handed over by other
 * apps don't.
 */
private fun Context.canReopen(uri: Uri): Boolean {
    if (uri.scheme == "file") return true
    try {
        // To write as well, where that was offered: for "Save".
        contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
        return true
    } catch (e: SecurityException) {
        // Reading alone, then.
    }
    return try {
        contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        true
    } catch (e: SecurityException) {
        false
    }
}

/**
 * Whether the document at [uri] may be written over, as far as can be told without trying: "Save"
 * is offered for these.
 *
 * Documents from the system file picker usually may, and those handed over by other apps mostly
 * not.
 */
private fun Context.canSaveOver(uri: Uri): Boolean {
    if (uri.scheme == "file") return uri.path?.let { File(it).canWrite() } == true
    return checkCallingOrSelfUriPermission(uri, Intent.FLAG_GRANT_WRITE_URI_PERMISSION) ==
        PackageManager.PERMISSION_GRANTED
}

/** Gives back read permissions for documents the start screen no longer offers. */
private fun Context.releaseUnused(store: DocumentStore) {
    val keep = store.reopenable()
    for (permission in contentResolver.persistedUriPermissions) {
        if (permission.uri.toString() !in keep) {
            val write =
                if (permission.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0
            contentResolver.releasePersistableUriPermission(
                permission.uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or write,
            )
        }
    }
}

/**
 * Hands the document at [uri] to Rust, which reads it straight from the file, or for a large one
 * maps it and reads only the parts it shows.
 *
 * Either way its bytes never pass through this app's own (much smaller) Java memory. Throws if it
 * can't be read.
 */
private fun Context.pdfFile(uri: Uri): PdfFile {
    val descriptor =
        checkNotNull(contentResolver.openFileDescriptor(uri, "r")) { "cannot open $uri" }
    // Rust takes the descriptor over and closes it.
    return PdfFile.fromFd(descriptor.detachFd())
}

/**
 * Opens [file], continuing where the reader left off if it was open before.
 *
 * A missing or wrong [password] leads to [Loaded.Locked]. Unless it ends there, [file] is closed:
 * an open document keeps its own hold on the bytes.
 */
private suspend fun Context.open(
    file: PdfFile,
    name: String,
    uri: Uri,
    password: String?,
    store: DocumentStore,
    drafts: DraftStore,
): Loaded {
    val opened = try {
        val cacheBytes = tileCacheBytes()
        val size = file.size().toLong()
        val key = withContext(Dispatchers.Default) {
            DocumentStore.fingerprint(size) { offset, length ->
                file.read(offset.toULong(), length.toUInt())
            }
        }
        val known = store.find(key)
        val wide = resources.configuration.screenWidthDp >= WIDE_WINDOW_DP
        val state = withContext(Dispatchers.Default) {
            PdfViewState(file, password, cacheBytes, known?.view, wide, known?.sidebar ?: true)
        }
        // Edits that weren't saved when the app was last ended are back, without asking.
        try {
            withContext(Dispatchers.IO) { state.restoreDraft(drafts, key) }
        } catch (e: PdfException) {
            Log.w("specto", "the draft of $name can't be restored", e)
            Toast.makeText(
                this,
                getString(R.string.draft_not_restored, name),
                Toast.LENGTH_LONG,
            ).show()
        }
        store.opened(key, name, uri.toString().takeIf { canReopen(uri) }, state.pageCount)
        releaseUnused(store)
        Loaded.Ready(state, name, key, size, file.isMapped())
    } catch (e: PdfException.PasswordRequired) {
        return Loaded.Locked(file, name, uri, wrongPassword = password != null)
    } catch (e: Exception) {
        Loaded.Failed(e.message ?: e.toString())
    }
    file.close()
    return opened
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun App(
    document: Uri?,
    store: DocumentStore,
    drafts: DraftStore,
    settings: AppSettings,
    crashLog: CrashLog,
    onDocument: (Uri?) -> Unit,
    /**
     * A document asked for from outside the app, to show when the open one can be left.
     * [onIncoming] takes note.
     */
    incoming: Uri?,
    onIncoming: () -> Unit,
    onReady: (Loaded.Ready?) -> Unit,
    /** Hands over what the volume keys should do, or null while no document is on screen. */
    onPageKeys: (((forward: Boolean) -> Unit)?) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val focusManager = LocalFocusManager.current
    var loaded by remember { mutableStateOf<Loaded?>(null) }
    // Counts the times a document was saved over, to read it afresh.
    var reloads by remember { mutableStateOf(0) }
    // Where the reader wants to go while the open document has unsaved edits.
    var leaving by remember { mutableStateOf<Leaving?>(null) }
    // What follows a "Save as" other than showing the saved document.
    var afterSaving by remember { mutableStateOf<(() -> Unit)?>(null) }
    var saving by remember { mutableStateOf(false) }
    // The copy for the print service is being written.
    var preparingToPrint by remember { mutableStateOf(false) }
    // "Discard changes" asks first.
    var discarding by remember { mutableStateOf(false) }

    /** Shows [next] (null: the start screen) instead, asking first if edits would be lost. */
    fun leaveFor(next: Uri?) {
        val unsaved = (loaded as? Loaded.Ready)?.state?.editState?.modified == true
        if (unsaved) leaving = Leaving(next) else onDocument(next)
    }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) leaveFor(uri)
    }
    val pick = { picker.launch(arrayOf("application/pdf")) }
    LaunchedEffect(incoming) {
        if (incoming != null) {
            leaveFor(incoming)
            onIncoming()
        }
    }
    var showPages by remember { mutableStateOf(false) }
    // Which tab the top-left button opens: the one used last, pages at first.
    var preferContents by remember { mutableStateOf(false) }
    var externalLink by remember { mutableStateOf<String?>(null) }
    // Tapping the page hides everything but the page, and tapping again brings the bars back.
    var fullScreen by remember { mutableStateOf(false) }
    var showGoTo by remember { mutableStateOf(false) }
    var showProperties by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }
    // The top bar is a search field for now.
    var searching by remember { mutableStateOf(false) }
    // Counts searches closed, so the page can take the keyboard back.
    var searchesClosed by remember { mutableStateOf(0) }
    fun closeSearch() {
        if (!searching) return
        searching = false
        searchesClosed++
        (loaded as? Loaded.Ready)?.state?.endSearch()
    }
    fun show(next: Loaded?) {
        (loaded as? Loaded.Ready)?.let {
            store.saveView(it.key, it.state.savedView(), it.state.sidebarOpen)
            it.keepDraft(drafts)
            it.state.close()
        }
        loaded = next
        onReady(next as? Loaded.Ready)
    }

    LaunchedEffect(document, reloads) {
        showPages = false
        fullScreen = false
        searching = false
        show(null)
        if (document == null) return@LaunchedEffect
        loaded = Loaded.Loading
        show(
            try {
                val name = context.displayName(document)
                val file = withContext(Dispatchers.IO) { context.pdfFile(document) }
                context.open(file, name, document, password = null, store, drafts)
            } catch (e: Exception) {
                // Moved, deleted, or the permission to read it is gone.
                store.unreachable(document.toString())
                Loaded.Failed((e as? PdfException.Io)?.reason ?: e.message ?: e.toString())
            },
        )
    }
    DisposableEffect(Unit) {
        onDispose { show(null) }
    }

    /**
     * Writes the open document, as edited, to [uri] and then shows what is there, where the reader
     * was, unless [then] says what to do instead.
     *
     * The document is read afresh from [uri], also if that is the file it was opened from.
     */
    fun saveTo(uri: Uri, then: (() -> Unit)?) {
        val open = loaded as? Loaded.Ready ?: return
        scope.launch {
            saving = true
            try {
                val written = context.writeEdited(open.state, open.name)
                val view = open.state.savedView()
                val sidebar = open.state.sidebarOpen
                val pages = open.state.pageCount
                if (open.mapped) {
                    // The reader may be saving over the file that is open, which is read where it
                    // lies: let go of it first.
                    show(null)
                    loaded = Loaded.Loading
                }
                try {
                    context.copyTo(written, uri)
                } catch (e: Exception) {
                    if (!open.mapped) throw e
                    // Nothing is open any more, but what was written is.
                    open.dropDraft(drafts)
                    Toast.makeText(context, R.string.could_not_save_there, Toast.LENGTH_LONG).show()
                    onDocument(Uri.fromFile(written))
                    return@launch
                }
                // The edits are in the file written. The one opened stays as it is on disk.
                open.dropDraft(drafts)
                // The saved file is a new document to the store: it starts out where this one is.
                val key = withContext(Dispatchers.IO) { written.fingerprint() }
                store.opened(
                    key,
                    context.displayName(uri),
                    uri.toString().takeIf {
                        context.canReopen(uri)
                    },
                    pages,
                )
                store.saveView(key, view, sidebar)
                withContext(Dispatchers.IO) { written.delete() }
                if (then != null) {
                    then()
                } else {
                    onDocument(uri)
                    reloads++
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val reason = (e as? PdfException.Save)?.reason ?: e.message ?: e.toString()
                Toast.makeText(
                    context,
                    context.getString(R.string.could_not_save, reason),
                    Toast.LENGTH_LONG,
                ).show()
            } finally {
                saving = false
            }
        }
    }

    /** "Print": the system's print dialog, for the document as it is now. */
    fun print() {
        val open = loaded as? Loaded.Ready ?: return
        if (preparingToPrint) return
        scope.launch {
            preparingToPrint = true
            try {
                context.printDocument(open.state, open.name)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val reason = (e as? PdfException.Save)?.reason ?: e.message ?: e.toString()
                Toast.makeText(
                    context,
                    context.getString(R.string.could_not_print, reason),
                    Toast.LENGTH_LONG,
                ).show()
            } finally {
                preparingToPrint = false
            }
        }
    }

    // "Save as": the system asks where.
    val saver =
        rememberLauncherForActivityResult(
            ActivityResultContracts.CreateDocument("application/pdf"),
        ) { uri ->
            val then = afterSaving
            afterSaving = null
            if (uri != null) saveTo(uri, then)
        }
    fun saveAs(name: String, then: (() -> Unit)? = null) {
        afterSaving = then
        saver.launch(name)
    }

    // "Save": over the file the document was opened from, where the app may write to it.
    val canSave = remember(document) { document != null && context.canSaveOver(document) }

    /**
     * Saves the open document over its file and goes on with [then].
     *
     * The document stays open as it is, edits still to be undone, unless its file is mapped: that
     * one has to be closed to be written, and is read afresh.
     */
    fun save(then: (() -> Unit)? = null) {
        val open = loaded as? Loaded.Ready ?: return
        val uri = document ?: return
        if (open.mapped) return saveTo(uri, then)
        scope.launch {
            saving = true
            try {
                // Written beside it first and then copied over it: should writing fail, the file is
                // as it was.
                val written = context.writeEdited(open.state, open.name)
                context.copyTo(written, uri)
                open.state.writtenIsSaved()
                // Its content is how the store knows a document: under what it is now, it is where
                // this one was.
                val (key, size) = withContext(Dispatchers.IO) {
                    val result = written.fingerprint() to written.length()
                    written.delete()
                    result
                }
                store.opened(
                    key,
                    open.name,
                    uri.toString().takeIf {
                        context.canReopen(uri)
                    },
                    open.state.pageCount,
                )
                store.saveView(key, open.state.savedView(), open.state.sidebarOpen)
                // The file it was a draft of is gone.
                drafts.delete(open.key)
                open.key = key
                open.bytes = size
                then?.invoke()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val reason = (e as? PdfException.Save)?.reason ?: e.message ?: e.toString()
                Toast.makeText(
                    context,
                    context.getString(R.string.could_not_save, reason),
                    Toast.LENGTH_LONG,
                ).show()
            } finally {
                saving = false
            }
        }
    }

    val ready = loaded as? Loaded.Ready
    // A wide window (a tablet, but not one sharing its screen) has room for Pages and Contents in a
    // sidebar next to the document. Narrower ones show them full screen instead, on request.
    val wide = LocalConfiguration.current.screenWidthDp >= WIDE_WINDOW_DP
    val overlay = showPages && !wide
    val immersive = fullScreen && ready != null && !overlay
    val sidebarShown = wide && ready != null && ready.state.sidebarOpen && !immersive
    // Two pages side by side need the room, too.
    ready?.state?.let { SideEffect { it.wideWindow = wide } }
    BackHandler(enabled = overlay) { showPages = false }
    BackHandler(enabled = immersive) { fullScreen = false }
    BackHandler(enabled = searching) { closeSearch() }
    // Pages picked in the page grid: Back lets go of them first.
    val picking = ready != null && ready.state.selectedPages.isNotEmpty()
    BackHandler(enabled = picking) { ready?.state?.selectedPages = emptySet() }
    if (ready != null && !overlay && !sidebarShown) {
        // Picked where the page grid is. Without it nothing is.
        SideEffect { ready.state.selectedPages = emptySet() }
    }
    StatusBar(hidden = immersive)

    // The sidebar shows the same thumbnails, so the strip would only repeat them.
    val hasStrip =
        ready != null && !overlay && !immersive && !sidebarShown && ready.state.pageCount > 1
    // The document whose strip is showing, kept while the strip slides out.
    var stripState by remember { mutableStateOf<PdfViewState?>(null) }
    if (hasStrip) {
        stripState = ready?.state
    } else if (ready == null) {
        stripState = null
    }

    if (showSettings) {
        // In place of everything else, while the document stays open underneath.
        BackHandler { showSettings = false }
        SideEffect { onPageKeys(null) }
        SettingsScreen(settings, crashLog, onBack = { showSettings = false })
        return
    }
    // The volume keys move through the document like Page Down and Page Up, while it is what the
    // reader sees.
    val pageKeys: ((Boolean) -> Unit)? =
        if (ready != null && !overlay) { forward -> ready.state.pageBy(scope, forward) } else null
    SideEffect { onPageKeys(pageKeys) }

    Scaffold(
        // The page view goes edge to edge: under the camera cutout (in landscape, beside it) and
        // under the navigation handle. Everything else keeps clear of them by itself.
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        // The bars slide in and out. They take up their space as they go, so the page area between
        // them resizes smoothly and the page glides rather than jumps.
        bottomBar = {
            AnimatedVisibility(
                visible = hasStrip,
                enter = expandVertically(tween(BARS_MS), expandFrom = Alignment.Top),
                exit = shrinkVertically(tween(BARS_MS), shrinkTowards = Alignment.Top),
            ) {
                // Still there while sliding out, when `ready` may be gone.
                stripState?.let { ThumbnailStrip(it) }
            }
        },
        topBar = {
            AnimatedVisibility(
                visible = !immersive,
                enter = expandVertically(tween(BARS_MS), expandFrom = Alignment.Bottom),
                exit = shrinkVertically(tween(BARS_MS), shrinkTowards = Alignment.Bottom),
            ) {
                if (picking && ready != null) {
                    SelectionBar(ready.state)
                } else if (searching && ready != null) {
                    SearchBar(ready.state, onClose = ::closeSearch)
                } else {
                    TopAppBar(
                        windowInsets = WindowInsets.safeDrawing.only(
                            WindowInsetsSides.Horizontal + WindowInsetsSides.Top,
                        ),
                        navigationIcon = {
                            if (ready != null) {
                                IconButton(
                                    onClick = {
                                        if (wide) {
                                            ready.state.sidebarOpen = !ready.state.sidebarOpen
                                        } else {
                                            showPages = !showPages
                                        }
                                    },
                                ) {
                                    if (overlay) {
                                        Icon(
                                            Icons.AutoMirrored.Filled.ArrowBack,
                                            contentDescription = stringResource(
                                                R.string.back_to_document,
                                            ),
                                        )
                                    } else {
                                        Icon(
                                            Icons.Filled.Menu,
                                            contentDescription = stringResource(
                                                R.string.contents_and_pages,
                                            ),
                                        )
                                    }
                                }
                            }
                        },
                        title = {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    ready?.name ?: stringResource(R.string.app_name),
                                    Modifier.weight(1f, fill = false),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                if (ready?.state?.editState?.modified == true) {
                                    // Changed, and not saved anywhere yet.
                                    Text(
                                        stringResource(R.string.edited),
                                        Modifier.padding(start = 8.dp),
                                        style = MaterialTheme.typography.labelLarge,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        maxLines = 1,
                                    )
                                }
                            }
                        },
                        actions = {
                            if (ready != null && !overlay) {
                                IconButton(onClick = { searching = true }) {
                                    Icon(
                                        Icons.Filled.Search,
                                        contentDescription = stringResource(R.string.search),
                                    )
                                }
                                // How this document is shown. Everything else is in the overflow menu.
                                ViewMenu(ready.state, wide)
                            }
                            var menu by remember { mutableStateOf(false) }
                            IconButton(onClick = { menu = true }) {
                                Icon(
                                    Icons.Filled.MoreVert,
                                    contentDescription = stringResource(R.string.more),
                                )
                            }
                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                DropdownMenuItem(
                                    text = { Text(stringResource(R.string.menu_open)) },
                                    onClick = {
                                        menu = false
                                        pick()
                                    },
                                )
                                if (ready != null && document != null) {
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.menu_share)) },
                                        onClick = {
                                            menu = false
                                            scope.launch {
                                                try {
                                                    if (ready.state.editState.modified) {
                                                        // What the reader sees, not what is on disk.
                                                        context.shareEdited(ready.state, ready.name)
                                                    } else {
                                                        context.shareDocument(document, ready.name)
                                                    }
                                                } catch (e: CancellationException) {
                                                    throw e
                                                } catch (e: Exception) {
                                                    // Moved or deleted since it was opened.
                                                    Toast.makeText(
                                                        context,
                                                        R.string.could_not_share,
                                                        Toast.LENGTH_LONG,
                                                    ).show()
                                                }
                                            }
                                        },
                                    )
                                }
                                if (ready != null) {
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.menu_print)) },
                                        onClick = {
                                            menu = false
                                            print()
                                        },
                                    )
                                    if (canSave && ready.state.editState.modified) {
                                        DropdownMenuItem(
                                            text = { Text(stringResource(R.string.save)) },
                                            onClick = {
                                                menu = false
                                                save()
                                            },
                                        )
                                    }
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.save_as)) },
                                        onClick = {
                                            menu = false
                                            saveAs(ready.name)
                                        },
                                    )
                                    if (ready.state.editState.modified) {
                                        DropdownMenuItem(
                                            text = {
                                                Text(stringResource(R.string.menu_discard_changes))
                                            },
                                            onClick = {
                                                menu = false
                                                discarding = true
                                            },
                                        )
                                    }
                                    val edits = ready.state.editState
                                    edits.undo?.let { step ->
                                        DropdownMenuItem(
                                            text = { Text(stringResource(undoLabel(step))) },
                                            onClick = {
                                                menu = false
                                                ready.state.undo()
                                            },
                                        )
                                    }
                                    edits.redo?.let { step ->
                                        DropdownMenuItem(
                                            text = { Text(stringResource(redoLabel(step))) },
                                            onClick = {
                                                menu = false
                                                ready.state.redo()
                                            },
                                        )
                                    }
                                    if (ready.state.hasForm) {
                                        DropdownMenuItem(
                                            text = {
                                                Text(
                                                    stringResource(R.string.menu_highlight_fields),
                                                )
                                            },
                                            // Ticked while they are.
                                            trailingIcon = {
                                                if (ready.state.highlightFields) {
                                                    Icon(Icons.Filled.Check, null)
                                                }
                                            },
                                            onClick = {
                                                menu = false
                                                ready.state.toggleHighlightFields()
                                            },
                                        )
                                        DropdownMenuItem(
                                            text = {
                                                Text(stringResource(R.string.menu_reset_form))
                                            },
                                            onClick = {
                                                menu = false
                                                ready.state.resetForm()
                                            },
                                        )
                                    }
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.menu_go_to_page)) },
                                        onClick = {
                                            menu = false
                                            showGoTo = true
                                        },
                                    )
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.menu_properties)) },
                                        onClick = {
                                            menu = false
                                            showProperties = true
                                        },
                                    )
                                }
                                DropdownMenuItem(
                                    text = { Text(stringResource(R.string.menu_settings)) },
                                    onClick = {
                                        menu = false
                                        showSettings = true
                                    },
                                )
                                if (document != null) {
                                    // Back to the start screen and its recent documents.
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.menu_close)) },
                                        onClick = {
                                            menu = false
                                            leaveFor(null)
                                        },
                                    )
                                }
                            }
                        },
                    )
                }
            }
        },
    ) { insets ->
        Box(
            Modifier
                .fillMaxSize()
                .padding(insets)
                .background(MaterialTheme.colorScheme.surfaceContainerHighest),
            contentAlignment = Alignment.Center,
        ) {
            val safe = Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(besideAndBelow))
            when (val l = loaded) {
                null -> StartScreen(
                    store.recents,
                    onOpen = pick,
                    onRecent = { entry -> entry.uri?.let { onDocument(Uri.parse(it)) } },
                    modifier = safe,
                )

                Loaded.Loading -> CircularProgressIndicator(safe)

                is Loaded.Failed -> Message(
                    stringResource(R.string.could_not_open),
                    l.message,
                    stringResource(R.string.open_another),
                    pick,
                    safe,
                )

                is Loaded.Locked -> {
                    Message(
                        stringResource(R.string.password_protected, l.name),
                        "",
                        stringResource(R.string.open_another),
                        pick,
                        safe,
                    )
                    // A fresh dialog for every attempt, so a wrong password doesn't stay in the
                    // field.
                    key(l) {
                        PasswordDialog(
                            name = l.name,
                            wrongPassword = l.wrongPassword,
                            onDismiss = {
                                l.file.close()
                                show(
                                    Loaded.Failed(
                                        context.getString(R.string.password_needed, l.name),
                                    ),
                                )
                            },
                            onPassword = { password ->
                                loaded = Loaded.Loading
                                scope.launch {
                                    show(
                                        context.open(
                                            l.file,
                                            l.name,
                                            l.uri,
                                            password,
                                            store,
                                            drafts,
                                        ),
                                    )
                                }
                            },
                        )
                    }
                }

                is Loaded.Ready -> {
                    val navigator: @Composable (Modifier, WindowInsetsSides, Boolean) -> Unit =
                        { modifier, insets, closeAfterwards ->
                            Navigator(
                                l.state,
                                startOnContents = preferContents,
                                onTab = { preferContents = it },
                                onPage = {
                                    l.state.goToPage(it)
                                    if (closeAfterwards) showPages = false
                                },
                                onOutlineEntry = {
                                    (l.state.openOutlineEntry(it) as? Activated.Uri)?.let { link ->
                                        externalLink = link.uri
                                    }
                                    if (closeAfterwards) showPages = false
                                },
                                modifier = modifier,
                                insets = insets,
                                // Beside the document, it keeps up with it.
                                follow = !closeAfterwards,
                            )
                        }
                    if (overlay) {
                        navigator(Modifier.fillMaxSize(), besideAndBelow, true)
                    } else {
                        Row(Modifier.fillMaxSize()) {
                            // The sidebar takes its width as it slides, so the page area resizes
                            // smoothly, as with the bars.
                            AnimatedVisibility(
                                visible = sidebarShown,
                                enter = expandHorizontally(
                                    tween(BARS_MS),
                                    expandFrom = Alignment.End,
                                ),
                                exit = shrinkHorizontally(
                                    tween(BARS_MS),
                                    shrinkTowards = Alignment.End,
                                ),
                            ) {
                                navigator(
                                    Modifier.width(
                                        SIDEBAR_WIDTH,
                                    ).fillMaxHeight().testTag(SIDEBAR_TAG),
                                    WindowInsetsSides.Start + WindowInsetsSides.Bottom,
                                    false,
                                )
                            }
                            Box(Modifier.weight(1f).fillMaxHeight()) {
                                PdfView(
                                    l.state,
                                    // The tag is how UI tests find the page area.
                                    Modifier.fillMaxSize().testTag(PAGE_VIEW_TAG),
                                    onExternalLink = { externalLink = it },
                                    // While searching, a tap puts the keyboard away instead of the
                                    // bars.
                                    onTap = {
                                        if (searching) {
                                            focusManager.clearFocus()
                                        } else {
                                            fullScreen = !fullScreen
                                        }
                                    },
                                    onSearch = {
                                        // The search field is part of the bars.
                                        fullScreen = false
                                        searching = true
                                    },
                                    onSave = {
                                        if (canSave && l.state.editState.modified) {
                                            save()
                                        } else {
                                            saveAs(
                                                l.name,
                                            )
                                        }
                                    },
                                    onPrint = ::print,
                                    refocus = searchesClosed,
                                )
                                // Always there in the normal view. In full screen it shows up on a
                                // page turn and leaves again.
                                var turned by remember(l.state) { mutableStateOf(false) }
                                LaunchedEffect(l.state, immersive) {
                                    turned = false
                                    snapshotFlow { l.state.currentPage }.drop(1).collectLatest {
                                        turned = true
                                        delay(INDICATOR_MS)
                                        turned = false
                                    }
                                }
                                // Fully qualified: inside the Row, the plain name would mean the
                                // variant that animates a row's child.
                                androidx.compose.animation.AnimatedVisibility(
                                    visible = !immersive || turned,
                                    modifier = Modifier
                                        .align(Alignment.BottomCenter)
                                        // Above the strip if there is one, else above the
                                        // navigation bar.
                                        .then(
                                            if (hasStrip) {
                                                Modifier
                                            } else {
                                                Modifier.navigationBarsPadding()
                                            },
                                        )
                                        .padding(12.dp),
                                    enter = fadeIn(),
                                    exit = fadeOut(),
                                ) {
                                    PageIndicator(l.state)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    externalLink?.let { link ->
        ExternalLinkDialog(link, onDismiss = { externalLink = null })
    }

    leaving?.let { asked ->
        if (ready == null) {
            // Nothing left to lose.
            SideEffect {
                leaving = null
                onDocument(asked.to)
            }
        } else {
            UnsavedChangesDialog(
                ready.name,
                saveAs = !canSave,
                onSave = {
                    leaving = null
                    if (canSave) {
                        save {
                            onDocument(asked.to)
                        }
                    } else {
                        saveAs(ready.name) { onDocument(asked.to) }
                    }
                },
                onDiscard = {
                    leaving = null
                    ready.dropDraft(drafts)
                    onDocument(asked.to)
                },
                onCancel = { leaving = null },
            )
        }
    }
    if (discarding && ready != null) {
        DiscardChangesDialog(
            ready.name,
            onDiscard = {
                discarding = false
                // Read afresh from the file, as it is on disk.
                ready.dropDraft(drafts)
                reloads++
            },
            onCancel = { discarding = false },
        )
    }
    if (saving) {
        BusyDialog(stringResource(R.string.saving))
    } else if (preparingToPrint) {
        BusyDialog(stringResource(R.string.preparing_to_print))
    }

    if (showProperties && ready != null) {
        PropertiesDialog(ready.state, ready.name, ready.bytes, onDismiss = {
            showProperties = false
        })
    }

    if (showGoTo && ready != null) {
        GoToPageDialog(
            ready.state,
            onDismiss = { showGoTo = false },
            onPage = {
                showGoTo = false
                showPages = false
                ready.state.goToPage(it)
            },
        )
    }
}

/**
 * Hides or shows the status bar, for full screen.
 *
 * The navigation bar stays: hiding it makes Android show its "viewing full screen" explanation and
 * turns going home into two swipes, for the sake of a thin handle.
 */
@Composable
private fun StatusBar(hidden: Boolean) {
    val view = LocalView.current
    LaunchedEffect(hidden) {
        val window = (view.context as? Activity)?.window ?: return@LaunchedEffect
        val bars = WindowCompat.getInsetsController(window, view)
        if (hidden) {
            bars.systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            bars.hide(WindowInsetsCompat.Type.statusBars())
        } else {
            bars.show(WindowInsetsCompat.Type.statusBars())
        }
    }
}

@Composable
private fun Message(
    title: String,
    detail: String,
    action: String,
    onAction: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        if (detail.isNotEmpty()) Text(detail, textAlign = TextAlign.Center)
        Button(onClick = onAction) { Text(action) }
    }
}

/**
 * What the top-left button opens, full screen or as a sidebar: the page grid and, if the document
 * has an outline, its contents as a second tab. The tabs can also be swiped.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Navigator(
    state: PdfViewState,
    startOnContents: Boolean,
    onTab: (contents: Boolean) -> Unit,
    onPage: (Int) -> Unit,
    onOutlineEntry: (Int) -> Unit,
    modifier: Modifier,
    /** Which screen edges it touches, to keep clear of cutouts and the navigation bar there. */
    insets: WindowInsetsSides,
    /** Whether the page list keeps the current page in view (shown beside the document). */
    follow: Boolean,
) {
    val background = modifier
        .background(MaterialTheme.colorScheme.surface)
        .windowInsetsPadding(WindowInsets.safeDrawing.only(insets))
    if (state.outline.isEmpty()) {
        PagesGrid(state, onPage, background, beside = follow)
        return
    }
    val scope = rememberCoroutineScope()
    val pager = rememberPagerState(initialPage = if (startOnContents) 1 else 0) { 2 }
    LaunchedEffect(pager.settledPage) { onTab(pager.settledPage == 1) }
    Column(background) {
        PrimaryTabRow(selectedTabIndex = pager.currentPage) {
            for ((index, title) in listOf(R.string.tab_pages, R.string.tab_contents).withIndex()) {
                Tab(
                    selected = pager.currentPage == index,
                    onClick = { scope.launch { pager.animateScrollToPage(index) } },
                    text = { Text(stringResource(title)) },
                )
            }
        }
        HorizontalPager(pager, Modifier.fillMaxSize()) { tab ->
            if (tab == 0) {
                PagesGrid(state, onPage, Modifier.fillMaxSize(), beside = follow)
            } else {
                OutlineList(state, onOutlineEntry, Modifier.fillMaxSize())
            }
        }
    }
}

/** Schemes a link in a document may open, after the user confirmed. */
private val OPENABLE_SCHEMES = setOf("http", "https", "mailto")

/**
 * A link leaving the document was tapped.
 *
 * Documents can link anywhere, so the address is shown and nothing opens without confirmation.
 */
@Composable
private fun ExternalLinkDialog(link: String, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val uri = remember(link) { Uri.parse(link) }
    val openable = uri.scheme?.lowercase() in OPENABLE_SCHEMES
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                stringResource(
                    if (openable) R.string.link_open_title else R.string.link_blocked_title,
                ),
            )
        },
        text = { Text(link, maxLines = 6, overflow = TextOverflow.Ellipsis) },
        confirmButton = {
            if (openable) {
                TextButton(
                    onClick = {
                        onDismiss()
                        try {
                            context.startActivity(Intent(Intent.ACTION_VIEW, uri))
                        } catch (e: ActivityNotFoundException) {
                            // No app for it, so nothing to do.
                        }
                    },
                ) { Text(stringResource(R.string.open)) }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(if (openable) R.string.cancel else R.string.close))
            }
        },
    )
}

@Composable
private fun PasswordDialog(
    name: String,
    wrongPassword: Boolean,
    onDismiss: () -> Unit,
    onPassword: (String) -> Unit,
) {
    var password by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.password_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.password_prompt, name))
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    modifier = Modifier.focusRequester(focus),
                    label = { Text(stringResource(R.string.password_label)) },
                    singleLine = true,
                    isError = wrongPassword,
                    supportingText = if (wrongPassword) {
                        { Text(stringResource(R.string.password_wrong)) }
                    } else {
                        null
                    },
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(
                        keyboardType = KeyboardType.Password,
                        imeAction = ImeAction.Done,
                    ),
                    keyboardActions = KeyboardActions(onDone = { onPassword(password) }),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onPassword(password) }) { Text(stringResource(R.string.open)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun GoToPageDialog(state: PdfViewState, onDismiss: () -> Unit, onPage: (Int) -> Unit) {
    var text by remember { mutableStateOf("") }
    val page = if (text.isBlank()) null else state.findPage(text)
    val last = state.pageCount - 1
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.go_to_title)) },
        text = {
            OutlinedTextField(
                value = text,
                // Printed page numbers can be anything ("iv", "A-3"). Plain ones are digits.
                onValueChange = { new ->
                    text = if (state.hasPageLabels) {
                        new.take(
                            20,
                        )
                    } else {
                        new.filter(Char::isDigit).take(9)
                    }
                },
                modifier = Modifier.focusRequester(focus),
                label = {
                    Text(
                        stringResource(
                            R.string.go_to_label,
                            state.pageLabel(0),
                            state.pageLabel(last),
                        ),
                    )
                },
                supportingText = if (state.hasPageLabels) {
                    {
                        // A typed number may be a printed number or a position.
                        Text(
                            if (page != null) {
                                stringResource(R.string.go_to_position, page + 1, state.pageCount)
                            } else {
                                stringResource(R.string.go_to_position_hint, state.pageCount)
                            },
                        )
                    }
                } else {
                    null
                },
                singleLine = true,
                isError = text.isNotBlank() && page == null,
                keyboardOptions = KeyboardOptions(
                    keyboardType =
                        if (state.hasPageLabels) KeyboardType.Ascii else KeyboardType.Number,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { page?.let(onPage) }),
            )
        },
        confirmButton = {
            TextButton(onClick = {
                page?.let(onPage)
            }, enabled = page != null) { Text(stringResource(R.string.go)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun PageIndicator(state: PdfViewState, modifier: Modifier = Modifier) {
    val current = state.currentPage
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.inverseSurface.copy(alpha = 0.8f),
        contentColor = MaterialTheme.colorScheme.inverseOnSurface,
    ) {
        // "4 / 120", or for a pair "4–5 / 120". With printed page numbers "iv (4 / 120)" and "iv–v
        // (4–5 / 120)".
        val last = state.lastPage
        val positions = if (last > current) "${current + 1}–${last + 1}" else "${current + 1}"
        Text(
            if (state.hasPageLabels) {
                val labels =
                    if (last > current) {
                        "${state.pageLabel(current)}–${state.pageLabel(last)}"
                    } else {
                        state.pageLabel(current)
                    }
                "$labels ($positions / ${state.pageCount})"
            } else {
                "$positions / ${state.pageCount}"
            },
            Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
            style = MaterialTheme.typography.labelLarge,
        )
    }
}
