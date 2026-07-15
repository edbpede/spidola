// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import dev.spidola.tv.core.corekit.PairingAccess
import dev.spidola.tv.core.corekit.PairingEvent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.PairingSession
import uniffi.core_api.PairingSubmission

@OptIn(ExperimentalCoroutinesApi::class)
class PairingViewModelTest {
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
    fun `a started session surfaces the address and token to show`() =
        runTest(dispatcher) {
            val access = FakePairingAccess()
            val viewModel = PairingViewModel(access, PairingHandoff(), host = { "192.168.1.40" })

            viewModel.start()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is PairingState.Ready)
            assertEquals("http://192.168.1.40:53219", state.session.url)
            assertEquals("482913", state.session.token)
        }

    @Test
    fun `the shell supplies the lan address rather than letting the core guess`() =
        runTest(dispatcher) {
            val access = FakePairingAccess()
            val viewModel = PairingViewModel(access, PairingHandoff(), host = { "10.0.0.7" })

            viewModel.start()
            advanceUntilIdle()

            // The core infers from the route out of the host, which is wrong behind a VPN — so a
            // `null` here would be the shell failing to do the one thing only it can.
            assertEquals("10.0.0.7", access.requestedHost)
        }

    @Test
    fun `a TV with no LAN address asks the core to infer, which fails loudly`() =
        runTest(dispatcher) {
            val access = FakePairingAccess(failure = ApiException.InvalidInput("no usable LAN address"))
            val viewModel = PairingViewModel(access, PairingHandoff(), host = { null })

            viewModel.start()
            advanceUntilIdle()

            assertNull(access.requestedHost)
            val state = viewModel.state.value
            check(state is PairingState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
        }

    @Test
    fun `a submission is offered to the form, never added`() =
        runTest(dispatcher) {
            val handoff = PairingHandoff()
            val access =
                FakePairingAccess(
                    submission =
                        PairingSubmission.Xtream(
                            server = "http://tv.example",
                            username = "viewer",
                            password = "hunter2",
                        ),
                )
            val viewModel = PairingViewModel(access, handoff, host = { "192.168.1.40" })

            viewModel.start()
            advanceUntilIdle()

            assertTrue(viewModel.submitted.value)
            val prefill = handoff.take()
            assertEquals(AddSourceMode.XTREAM, prefill?.mode)
            assertEquals("http://tv.example", prefill?.server)
            assertEquals("viewer", prefill?.username)
            assertEquals("hunter2", prefill?.password)
        }

    @Test
    fun `an m3u submission pre-fills the url flow`() =
        runTest(dispatcher) {
            val handoff = PairingHandoff()
            val access = FakePairingAccess(submission = PairingSubmission.M3uUrl(url = "http://x/list.m3u"))
            val viewModel = PairingViewModel(access, handoff, host = { "192.168.1.40" })

            viewModel.start()
            advanceUntilIdle()

            val prefill = handoff.take()
            assertEquals(AddSourceMode.URL, prefill?.mode)
            assertEquals("http://x/list.m3u", prefill?.url)
        }

    @Test
    fun `stopping ends the collection, which is what stops the server`() =
        runTest(dispatcher) {
            val access = FakePairingAccess()
            val viewModel = PairingViewModel(access, PairingHandoff(), host = { "192.168.1.40" })

            viewModel.start()
            advanceUntilIdle()
            assertTrue(access.collecting, "expected the pairing flow to be under collection")

            viewModel.stop()
            advanceUntilIdle()

            // The server's lifetime is the collector's lifetime: no collection, no server.
            assertFalse(access.collecting, "stopping the screen must end the collection")
        }

    @Test
    fun `the handoff yields a submission once`() {
        val handoff = PairingHandoff()
        handoff.offer(PairingSubmission.M3uUrl(url = "http://x/list.m3u"))

        assertEquals("http://x/list.m3u", handoff.take()?.url)
        // Re-entering add-source later must not re-fill someone else's account.
        assertNull(handoff.take())
    }
}

/**
 * A fake [PairingAccess]: records the host it was asked to advertise and whether its flow is still
 * being collected, which is the property that stands in for "the server is running".
 */
private class FakePairingAccess(
    private val submission: PairingSubmission? = null,
    private val failure: ApiException? = null,
) : PairingAccess {
    var requestedHost: String? = null
        private set
    var collecting: Boolean = false
        private set

    override fun pair(host: String?): Flow<PairingEvent> =
        flow {
            requestedHost = host
            failure?.let { throw it }
            collecting = true
            try {
                emit(
                    PairingEvent.Started(
                        PairingSession(url = "http://$host:53219", port = 53219u, token = "482913"),
                    ),
                )
                submission?.let { emit(PairingEvent.Submitted(it)) }
                // The core never completes this stream; only the collector going away does.
                awaitCancellation()
            } finally {
                collecting = false
            }
        }
}
