// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.graphics.Bitmap
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.assertTextContains
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

    @Test
    fun coldLaunchSeedsFixtureAndMovesFocusDown() {
        composeRule.waitUntil(timeoutMillis = STARTUP_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(CHANNEL_ONE_TAG)).fetchSemanticsNodes().size == 1
        }
        composeRule
            .onNodeWithTag(CHANNEL_ONE_TAG)
            .assertTextContains("Channel 1")
            .assertIsDisplayed()
            .assertIsFocused()

        composeRule.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        composeRule.waitUntil(timeoutMillis = FOCUS_TIMEOUT_MS) {
            composeRule.onAllNodes(hasTestTag(CHANNEL_TWO_TAG) and isFocused()).fetchSemanticsNodes().size == 1
        }

        composeRule.onNodeWithTag(CHANNEL_ONE_TAG).assertIsNotFocused()
        composeRule
            .onNodeWithTag(CHANNEL_TWO_TAG)
            .assertTextContains("Channel 2")
            .assertIsDisplayed()
            .assertIsFocused()
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
        const val CHANNEL_ONE_TAG = "channel-0"
        const val CHANNEL_TWO_TAG = "channel-1"
        const val SCREENSHOT_FILE_NAME = "channel-2-focused.png"
        const val STARTUP_TIMEOUT_MS = 30_000L
        const val FOCUS_TIMEOUT_MS = 5_000L
    }
}
