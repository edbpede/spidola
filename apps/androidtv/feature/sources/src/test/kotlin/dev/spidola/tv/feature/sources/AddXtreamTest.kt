// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import dev.spidola.tv.core.corekit.ImportEvent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.ImportOutcome

@OptIn(ExperimentalCoroutinesApi::class)
class AddXtreamTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `an account is added then its catalog imported`() =
        runTest(dispatcher) {
            val access =
                FakeSourcesAccess(
                    importResult =
                        ImportEvent.Complete(
                            ImportOutcome(
                                inserted = 4210uL,
                                duplicatesDropped = 0uL,
                                emitted = 4210uL,
                                skipped = 0uL,
                                invalid = 0uL,
                            ),
                        ),
                )
            val viewModel = AddSourceViewModel(access)

            viewModel.submit(xtreamForm())
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is AddSourceState.Done)
            assertEquals(4210uL, state.outcome.inserted)
        }

    @Test
    fun `the password reaches the core verbatim and untrimmed`() =
        runTest(dispatcher) {
            val access = FakeSourcesAccess()
            val viewModel = AddSourceViewModel(access)

            // Surrounding space can be part of a password; trimming it would reject a valid account
            // and blame the user. The name and server are trimmed; the credential is not.
            viewModel.submit(xtreamForm(name = "  Home  ", server = "  http://tv.example  ", password = " hunter2 "))
            advanceUntilIdle()

            val call = access.xtreamCall
            assertNotNull(call)
            assertEquals("Home", call?.name)
            assertEquals("http://tv.example", call?.server)
            assertEquals(" hunter2 ", call?.password)
        }

    @Test
    fun `a rejected account surfaces an actionable error on the add screen`() =
        runTest(dispatcher) {
            // The core verifies before storing, so a wrong password comes back from `addXtream`
            // itself — it belongs here as a sentence, not on some later refresh as a mystery.
            val access = FakeSourcesAccess(addXtreamFailure = ApiException.Unauthorized())
            val viewModel = AddSourceViewModel(access)

            viewModel.submit(xtreamForm())
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is AddSourceState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
            // Nothing was stored, so there is no half-added source to clean up.
            assertTrue(access.deletedIds.isEmpty(), "a rejected account must not create a source")
        }

    @Test
    fun `an expired account is the same rejection, and says what to press`() =
        runTest(dispatcher) {
            // The 401-renewal path: a banned or expired account also answers Unauthorized, and the
            // prescribed action is re-entering the credentials (PRD §6.3).
            val access = FakeSourcesAccess(addXtreamFailure = ApiException.Unauthorized())
            val viewModel = AddSourceViewModel(access)

            viewModel.submit(xtreamForm())
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is AddSourceState.Failed)
            assertEquals(dev.spidola.tv.core.corekit.ErrorAction.FIX_INPUT, state.error.primaryAction)
        }

    @Test
    fun `each account field is required`() =
        runTest(dispatcher) {
            val viewModel = AddSourceViewModel(FakeSourcesAccess())

            listOf(
                xtreamForm(server = ""),
                xtreamForm(username = ""),
                xtreamForm(password = ""),
                xtreamForm(name = ""),
            ).forEach { form ->
                viewModel.submit(form)
                advanceUntilIdle()
                assertNotNull(viewModel.validation.value, "expected a complaint for $form")
                assertEquals(AddSourceState.Editing, viewModel.state.value)
            }
        }

    @Test
    fun `a blank-field complaint never reaches the core`() =
        runTest(dispatcher) {
            val access = FakeSourcesAccess()
            val viewModel = AddSourceViewModel(access)

            viewModel.submit(xtreamForm(password = ""))
            advanceUntilIdle()

            // Whether the account *works* is the core's answer; whether the form is filled in is not.
            assertEquals(null, access.xtreamCall)
        }

    private fun xtreamForm(
        name: String = "Home",
        server: String = "http://tv.example",
        username: String = "viewer",
        password: String = "hunter2",
    ) = AddSourceForm(
        mode = AddSourceMode.XTREAM,
        name = name,
        url = "",
        content = "",
        userAgent = "",
        acceptInvalidTls = false,
        server = server,
        username = username,
        password = password,
    )
}
