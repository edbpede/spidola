// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import uniffi.core_api.Handshake
import kotlin.test.Test
import kotlin.test.assertFailsWith

class SpidolaApplicationTest {
    @Test
    fun `current core contract is accepted`() {
        requireCompatibleCore(handshake(schema = 3u, boundary = 7u))
    }

    @Test
    fun `stale schema is rejected before bootstrap`() {
        assertFailsWith<IllegalStateException> {
            requireCompatibleCore(handshake(schema = 2u, boundary = 7u))
        }
    }

    @Test
    fun `stale boundary is rejected before bootstrap`() {
        assertFailsWith<IllegalStateException> {
            requireCompatibleCore(handshake(schema = 3u, boundary = 6u))
        }
    }

    @Test
    fun `frozen schema two boundary four shell is rejected`() {
        assertFailsWith<IllegalStateException> {
            requireCompatibleCore(handshake(schema = 2u, boundary = 4u))
        }
    }

    private fun handshake(
        schema: UInt,
        boundary: UInt,
    ): Handshake =
        Handshake(
            coreVersion = "0.0.0-test",
            coreGitRevision = "test",
            schemaVersion = schema,
            boundaryVersion = boundary,
        )
}
