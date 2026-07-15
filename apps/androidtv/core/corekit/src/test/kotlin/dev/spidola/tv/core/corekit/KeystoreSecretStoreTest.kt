// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import org.junit.jupiter.api.Assertions.assertDoesNotThrow
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import java.security.GeneralSecurityException

class KeystoreSecretStoreTest {
    @Test
    fun `a refused durable write crosses the callback as an internal error`() {
        assertThrows(ApiException.Internal::class.java) {
            executeSecretMutation { false }
        }
    }

    @Test
    fun `a platform crypto failure crosses the callback as an internal error`() {
        val failure =
            assertThrows(ApiException.Internal::class.java) {
                executeSecretMutation { throw GeneralSecurityException("keystore unavailable") }
            }

        assertInstanceOf(GeneralSecurityException::class.java, failure.cause)
    }

    @Test
    fun `a committed mutation succeeds`() {
        assertDoesNotThrow {
            executeSecretMutation { true }
        }
    }
}
