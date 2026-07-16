<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Top Shelf boundary

The configured TV Services extension reads a bounded, versioned favorites snapshot from the shared
`group.dev.spidola.tv` container. The main app writes that projection after startup and whenever it
enters the background, then asks TV Services to reload it. The snapshot contains stable identities
and display metadata only; stream locators, request headers, credentials, and guide state remain in
the core-owned database.
