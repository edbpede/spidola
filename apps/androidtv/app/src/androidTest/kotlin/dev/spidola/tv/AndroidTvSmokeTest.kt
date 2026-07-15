// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.graphics.Bitmap
import android.view.KeyEvent
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.pressKey
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.platform.io.PlatformTestStorageRegistry
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.name
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID

@RunWith(AndroidJUnit4::class)
@OptIn(ExperimentalTestApi::class)
class AndroidTvSmokeTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private val testSourceName = "Instrumentation Source-${UUID.randomUUID()}"
    private var createdSourceId: Long? = null

    @After
    fun removeInstrumentationSource() {
        val app =
            InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as SpidolaApplication
        runBlocking {
            createdSourceId?.let { app.container.core.deleteSource(it) }
        }
    }

    /** Drives the Phase 4 drill-down on the seeded fixture catalog: home → source → category →
     * channels, asserting D-pad focus lands and moves with the Test-Card Amber treatment. */
    @Test
    fun coldLaunchSeedsFixtureDrillDownAndMovesFocus() {
        // Home: the fixture source is the first focusable element.
        composeRule.waitUntil(timeoutMillis = STARTUP_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCE_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(SOURCE_TAG).assertIsFocused()
        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionCenter) }

        // Categories: the fixture playlist has one group.
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(GROUP_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(GROUP_TAG).assertIsFocused()
        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionCenter) }

        // Channels: the first channel is focused; D-pad down moves to the second.
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(CHANNEL_ONE_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(CHANNEL_ONE_TAG).assertIsDisplayed().assertIsFocused()

        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        composeRule.waitUntil(timeoutMillis = FOCUS_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(CHANNEL_TWO_TAG) and isFocused()).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(CHANNEL_TWO_TAG).assertIsDisplayed().assertIsFocused()
        retainChannelTwoScreenshot()
    }

    /** Typing must not trap a TV remote inside the first form field. */
    @Test
    fun addSourceFormKeepsDpadNavigationAfterTyping() {
        composeRule.waitUntil(timeoutMillis = STARTUP_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCE_TAG)).fetchSemanticsNodes().size == 1
        }
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Search channels")
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Add or manage sources")
        pressRemoteKey(KeyEvent.KEYCODE_ENTER)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCES_ADD_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(SOURCES_ADD_TAG).performSemanticsAction(SemanticsActions.OnClick)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasText("Paste a playlist")).fetchSemanticsNodes().size == 1
        }
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(NAME_TAG), useUnmergedTree = true).fetchSemanticsNodes().size == 1
        }

        composeRule
            .onNodeWithTag(NAME_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput("Live IPTV")
        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        composeRule.onNodeWithTag(URL_TAG, useUnmergedTree = true).assertIsFocused()

        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        composeRule.onNodeWithTag(USER_AGENT_TAG, useUnmergedTree = true).assertIsFocused()

        composeRule.onRoot().performKeyInput {
            pressKey(Key.DirectionDown)
            pressKey(Key.DirectionDown)
        }
        composeRule.onNodeWithTag(SUBMIT_TAG, useUnmergedTree = true).assertIsFocused()
    }

    /** Credential-bearing source fields must not survive saved-state restoration. */
    @Test
    fun addSourceCredentialsAreNotRestored() {
        composeRule.waitUntil(timeoutMillis = STARTUP_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCE_TAG)).fetchSemanticsNodes().size == 1
        }
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Search channels")
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Add or manage sources")
        pressRemoteKey(KeyEvent.KEYCODE_ENTER)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCES_ADD_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(SOURCES_ADD_TAG).performSemanticsAction(SemanticsActions.OnClick)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(NAME_TAG), useUnmergedTree = true).fetchSemanticsNodes().size == 1
        }

        composeRule
            .onNodeWithTag(NAME_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(RESTORATION_CONTROL_NAME)
        composeRule
            .onNodeWithTag(URL_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(CREDENTIAL_URL_CANARY)
        composeRule
            .onNodeWithTag(USER_AGENT_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(USER_AGENT_CANARY)

        composeRule.activityRule.scenario.recreate()
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(URL_TAG), useUnmergedTree = true).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(NAME_TAG, useUnmergedTree = true).assertTextEquals(RESTORATION_CONTROL_NAME)
        composeRule.onNodeWithTag(URL_TAG, useUnmergedTree = true).assertTextEquals("")
        composeRule.onNodeWithTag(USER_AGENT_TAG, useUnmergedTree = true).assertTextEquals("")

        composeRule.onNode(hasText("Paste a playlist")).performSemanticsAction(SemanticsActions.OnClick)
        composeRule
            .onNodeWithTag(CONTENT_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(CONTENT_CANARY)
        composeRule.activityRule.scenario.recreate()
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(CONTENT_TAG), useUnmergedTree = true).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(NAME_TAG, useUnmergedTree = true).assertTextEquals(RESTORATION_CONTROL_NAME)
        composeRule.onNodeWithTag(CONTENT_TAG, useUnmergedTree = true).assertTextEquals("")
    }

    /** A completed import must appear in both the retained manage screen and Home without restart. */
    @Test
    fun completedImportReloadsRetainedSourceLists() {
        composeRule.waitUntil(timeoutMillis = STARTUP_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCE_TAG)).fetchSemanticsNodes().size == 1
        }
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Search channels")
        pressRemoteKey(KeyEvent.KEYCODE_DPAD_DOWN)
        composeRule.waitForFocus("Add or manage sources")
        pressRemoteKey(KeyEvent.KEYCODE_ENTER)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(SOURCES_ADD_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag(SOURCES_ADD_TAG).performSemanticsAction(SemanticsActions.OnClick)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasText("Paste a playlist")).fetchSemanticsNodes().size == 1
        }
        composeRule.onNode(hasText("Paste a playlist")).performSemanticsAction(SemanticsActions.OnClick)
        composeRule
            .onNodeWithTag(NAME_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(testSourceName)
        composeRule
            .onNodeWithTag(CONTENT_TAG, useUnmergedTree = true)
            .performClick()
            .performTextInput(TEST_PLAYLIST)
        composeRule.onNodeWithTag(SUBMIT_TAG).performSemanticsAction(SemanticsActions.OnClick)

        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(DONE_TAG)).fetchSemanticsNodes().size == 1
        }
        val app =
            InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as SpidolaApplication
        createdSourceId =
            runBlocking {
                app.container.core.sources().single { it.name == testSourceName }.id
            }
        composeRule.onNodeWithTag(DONE_TAG).performSemanticsAction(SemanticsActions.OnClick)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag("manage-source-$testSourceName")).fetchSemanticsNodes().size == 1
        }

        pressRemoteKey(KeyEvent.KEYCODE_BACK)
        composeRule.waitUntil(timeoutMillis = NAV_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag("source-$testSourceName")).fetchSemanticsNodes().size == 1
        }
        composeRule.onNodeWithTag("source-$testSourceName").assertIsDisplayed()
    }

    private fun retainChannelTwoScreenshot() {
        val screenshot = requireNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        PlatformTestStorageRegistry
            .getInstance()
            .openOutputFile(SCREENSHOT_FILE_NAME)
            .use { output -> check(screenshot.compress(Bitmap.CompressFormat.PNG, 100, output)) }
    }

    private fun pressRemoteKey(keyCode: Int) {
        InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(keyCode)
    }

    private fun androidx.compose.ui.test.junit4.AndroidComposeTestRule<*, *>.waitForFocus(text: String) {
        waitUntil(timeoutMillis = FOCUS_TIMEOUT_MS) {
            onAllNodes(hasText(text) and isFocused()).fetchSemanticsNodes().size == 1
        }
    }

    private companion object {
        const val SOURCE_TAG = "source-Fixture Catalog"
        const val GROUP_TAG = "group-Fixture"
        const val CHANNEL_ONE_TAG = "channel-Channel 1"
        const val CHANNEL_TWO_TAG = "channel-Channel 2"
        const val NAME_TAG = "add-source-name"
        const val SOURCES_ADD_TAG = "sources-add"
        const val URL_TAG = "add-source-url"
        const val USER_AGENT_TAG = "add-source-userAgent"
        const val CONTENT_TAG = "add-source-content"
        const val SUBMIT_TAG = "add-source-submit"
        const val DONE_TAG = "add-source-done"
        const val RESTORATION_CONTROL_NAME = "Restoration control"
        const val CREDENTIAL_URL_CANARY = "https://example.test/list?password=credential-canary"
        const val USER_AGENT_CANARY = "Token credential-canary"
        const val CONTENT_CANARY =
            "#EXTM3U\n#EXTINF:-1,Canary\nhttps://example.test/live?token=credential-canary"
        const val TEST_PLAYLIST =
            "#EXTM3U\n#EXTINF:-1 group-title=\"Test\",Test Channel\nhttps://example.test/stream"
        const val SCREENSHOT_FILE_NAME = "channel-2-focused.png"
        const val STARTUP_TIMEOUT_MS = 30_000L
        const val NAV_TIMEOUT_MS = 10_000L
        const val FOCUS_TIMEOUT_MS = 5_000L
    }
}
