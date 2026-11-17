package net.xfbs.specto

import android.view.KeyEvent
import android.view.WindowManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Settings, from the overflow menu: what holds for the whole app, and what the switches there do.
 */
@RunWith(AndroidJUnit4::class)
class SettingsTest : AppTest() {
    private fun openPages() = open("pages.pdf", shows = "1 / 12")

    /** Flips the switch called [name] in Settings and returns to the document. */
    private fun flip(name: String) {
        menu("Settings")
        compose.onNodeWithText(name).performClick()
        compose.onNodeWithContentDescription("Back").performClick()
        waitUntilGone("Settings")
    }

    private fun keepsScreenOn(): Boolean {
        var flags = 0
        scenario!!.onActivity { flags = it.window.attributes.flags }
        return flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON != 0
    }

    private fun press(key: Int) {
        // Keys go to the window that has the focus: not before a closing menu gave it back.
        compose.waitUntil(WAIT_MS) {
            var focused = false
            scenario!!.onActivity { focused = it.hasWindowFocus() }
            focused
        }
        instrumentation.sendKeyDownUpSync(key)
        compose.waitForIdle()
    }

    @Test
    fun switchesAreOffAtFirstAndStayAsSet() {
        openPages()
        menu("Settings")
        compose.onNodeWithText("Keep the screen on").assertIsOff()
        compose.onNodeWithText("Volume keys turn pages").assertIsOff().performClick()
        compose.onNodeWithText("Volume keys turn pages").assertIsOn()
        // Back returns to the document, where it was.
        compose.onNodeWithContentDescription("Back").performClick()
        waitForText("1 / 12")
        // Kept for the next time the app is opened.
        reopen("pages.pdf")
        waitForText("1 / 12")
        menu("Settings")
        compose.onNodeWithText("Volume keys turn pages").assertIsOn()
        compose.onNodeWithText("Keep the screen on").assertIsOff()
    }

    @Test
    fun theScreenIsKeptOnWhileReadingIfAskedTo() {
        openPages()
        assertEquals(false, keepsScreenOn())
        flip("Keep the screen on")
        assertEquals(true, keepsScreenOn())
        // Only while there is something to read.
        menu("Close")
        waitForText("Open PDF")
        assertEquals(false, keepsScreenOn())
    }

    @Test
    fun volumeKeysTurnPagesIfAskedTo() {
        openPages()
        flip("Volume keys turn pages")
        press(KeyEvent.KEYCODE_VOLUME_DOWN)
        waitForText("2 / 12")
        press(KeyEvent.KEYCODE_VOLUME_DOWN)
        waitForText("3 / 12")
        press(KeyEvent.KEYCODE_VOLUME_UP)
        waitForText("2 / 12")
        // Switched off again, they are the system's.
        flip("Volume keys turn pages")
        press(KeyEvent.KEYCODE_VOLUME_UP)
        compose.onNodeWithText("2 / 12").assertIsDisplayed()
    }

    @Test
    fun theCrashLogCanBeReadAndDeleted() {
        val log = File(instrumentation.targetContext.filesDir, SpectoApp.CRASH_LOG_FILE)
        openPages()
        menu("Settings")
        compose.onNodeWithText("Nothing went wrong so far").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        waitForText("1 / 12")

        // As the Rust library records a panic.
        log.writeText(
            "--- 2026-11-15T12:00:00Z Rust panic\nthread 'render': number 7 went wrong\nat src/lib.rs:1:2\n\n",
        )
        menu("Settings")
        compose.onNodeWithText("Last entry", substring = true).assertIsDisplayed().performClick()
        // Which version of the app it is from goes with it, as the installed app names it.
        val target = instrumentation.targetContext
        val version = target.packageManager.getPackageInfo(target.packageName, 0).versionName
        compose.onNodeWithTag(CRASH_LOG_TAG)
            .assertTextContains("number 7 went wrong", substring = true)
            .assertTextContains("Specto $version (", substring = true)
        compose.onNodeWithText("Delete").performClick()
        waitForText("Nothing went wrong so far")
        assertEquals(false, log.exists())
    }
}
