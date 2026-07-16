// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class AboutScreenTest {
    @Test
    fun `licensee report parser uses generated coordinates and licenses`() {
        val report =
            """
            [
              {
                "groupId": "z.example",
                "artifactId": "player",
                "version": "2.0",
                "spdxLicenses": [{"identifier":"MIT","name":"MIT License","url":"https://example.test/mit"}],
                "scm": {"url":"https://example.test/source"}
              },
              {
                "groupId": "a.example",
                "artifactId": "core",
                "version": "1.0",
                "spdxLicenses": [{"identifier":"Apache-2.0","name":"Apache License 2.0"}]
              }
            ]
            """.trimIndent()

        val artifacts = parseLicenseeReport(report)

        assertEquals(listOf("a.example:core:1.0", "z.example:player:2.0"), artifacts.map { it.coordinate })
        assertEquals("MIT License — https://example.test/mit", artifacts.last().licenseNotices)
    }
}
