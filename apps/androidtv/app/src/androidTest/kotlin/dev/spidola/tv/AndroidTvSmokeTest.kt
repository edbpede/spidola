// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.graphics.Bitmap
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.platform.io.PlatformTestStorageRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@OptIn(ExperimentalTestApi::class)
class AndroidTvSmokeTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

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

    private fun retainChannelTwoScreenshot() {
        val screenshot = requireNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        PlatformTestStorageRegistry
            .getInstance()
            .openOutputFile(SCREENSHOT_FILE_NAME)
            .use { output -> check(screenshot.compress(Bitmap.CompressFormat.PNG, 100, output)) }
    }

    private companion object {
        const val SOURCE_TAG = "source-Fixture Catalog"
        const val GROUP_TAG = "group-Fixture"
        const val CHANNEL_ONE_TAG = "channel-Channel 1"
        const val CHANNEL_TWO_TAG = "channel-Channel 2"
        const val SCREENSHOT_FILE_NAME = "channel-2-focused.png"
        const val STARTUP_TIMEOUT_MS = 30_000L
        const val NAV_TIMEOUT_MS = 10_000L
        const val FOCUS_TIMEOUT_MS = 5_000L
    }
}
