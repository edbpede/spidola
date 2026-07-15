<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Xtream fixtures

Xtream Codes API responses (numbers-as-strings, missing fields, and other real-world
weirdness) that pin the defensive wire deserialization (`core-xtream`, Phase 6). Credentials
are removed before a capture is committed.

## Provenance convention

The M3U corpus carries its provenance note inline as a `#` comment. JSON has no comment
syntax — and half of these fixtures are top-level arrays, which cannot hold a note key
either — so the notes live here instead, one section per file. Licensing is declared for
`fixtures/**/*.json` in the repository's `REUSE.toml` for the same reason.

Each note states what the fixture exercises, where it came from, and what was scrubbed.

## Provenance of this corpus

**These are hand-authored, not captured.** They are modelled on the publicly documented
`player_api.php` response shapes and on quirks reported against real panels, but no Xtream
account was contacted to produce them, so nothing needed scrubbing beyond the placeholder
credentials noted below. They are therefore honest about *shape* and *quirks*, and silent
about *scale* and *unknown-unknowns* — a real scrubbed capture from a live headend is worth
adding on top of these the first time one is available, not instead of them.

Where a fixture carries credentials (`user_info` mirrors the account back, which real
headends do), the values are the literal placeholders `demo_user` / `demo_password`. No
fixture contains a real credential, and none ever should: the `user_info` block is exactly
where one would hide, which is why `core-xtream`'s DTO declines to declare those fields at
all.

## The fixtures

### `handshake-active.json`

A healthy account. **Exercises:** `auth` as a number; `exp_date` / `max_connections` /
`active_cons` as numbers-as-strings; the echoed `username`/`password` the DTO must drop
(TECH_SPEC §12); and the `server_info` block the client ignores entirely.

### `handshake-denied.json`

Rejected credentials. **Exercises:** the minimal refusal real panels send — `auth: 0` and
nothing else, with no `status` to lean on.

### `handshake-expired.json`

A lapsed subscription. **Exercises:** `auth: 1` with `status: "Expired"` — the case proving
`auth` alone is not enough to call an account healthy — plus `active_cons` as a bare number
where `handshake-active.json` sends a string.

### `live-categories.json`

**Exercises:** `category_id` as both string and number; `parent_id` in both spellings; and
two unusable rows (blank name, missing id) that must be dropped without failing the list.

### `live-streams.json`

The messy one. **Exercises:** `stream_id` as number and string; three channels sharing an
`epg_channel_id` (the SD/HD/4K case that forbids keying identity on it); `stream_icon` and
`epg_channel_id` as `""` and `null`; `tv_archive` as `1`, `"0"`, and `false`; an explicit
`container_extension` on a live row; and five unusable rows — empty id, zero id, blank name,
absent name, and an id that is a container — each of which must be skipped and counted.

### `vod-streams.json`

**Exercises:** `container_extension` present (`mkv`), uppercase (`MP4`, which must
normalize), and blank (falling back to the VOD default); `rating` as string, number, and
zero; a nonsense container (`mp4?token=x`) that must be refused; and a row whose
`category_id` matches no known category.

### `series.json`

**Exercises:** `series_id` as number and string; `backdrop_path` as an array the DTO ignores;
`cover`/`plot` as `""`; and two unusable rows (missing id, zero id).

### `series-info.json`

The documented episode shape: an **object keyed by season number as a string**.
**Exercises:** `id` as string and number; `episode_num` and `season` in both spellings; a
season-`0` row keyed under `"2"` (the key must win); a blank `title` that must earn a derived
name rather than a skip; `"info": []` (PHP's empty-array-for-empty-object); a `seasons` block
the client does not need; and an episode with an empty id that must be skipped.

### `series-info-array.json`

The other episode shape: some panels serialize a PHP array with contiguous integer keys, so
`episodes` arrives as a **JSON array of arrays** with the season numbers gone.
**Exercises:** the array shape, and the fallback to each episode's own `season` field that it
forces.
