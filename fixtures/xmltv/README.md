<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# XMLTV fixtures

Golden XMLTV corpus for the streaming EPG parser (`core-parse/xmltv`). Every fixture is
synthetic, contains no provider data, and is licensed with the repository under
AGPL-3.0-or-later.

- `basic.xml` — hand-authored from the XMLTV format description. Covers UTC timestamps,
  numeric offsets, entities, CDATA, optional descriptions, chunk boundaries, and one programme
  outside the rolling window.
- `messy.xml` — hand-authored hostile-but-well-delimited input. Covers a valid row followed by
  missing required content and an invalid calendar timestamp so skip accounting is stable.

Raw malformed UTF-8 and oversized token cases are constructed as bytes in the Rust tests because
neither is representable faithfully in a text fixture. Property tests mutate both fixtures and
also feed arbitrary byte vectors through arbitrary chunk boundaries.
