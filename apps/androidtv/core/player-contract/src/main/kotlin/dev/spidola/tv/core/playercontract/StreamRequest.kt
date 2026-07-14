// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

/**
 * A stream header override applied at playback time. Token-bearing values arrive from the
 * host-secrets callback at play time and are never persisted here (TECH_SPEC §12).
 */
data class StreamHeader(
    val name: String,
    val value: String,
)

/**
 * How much latency the viewer trades for resilience. Engine-neutral by construction: the settings
 * screen speaks this vocabulary, and each engine maps it onto its own knobs, so settings language
 * never names an engine (TECH_SPEC §8).
 */
enum class BufferingProfile(
    /** The couch-legible label (PRD §8.6 voice) — describes the trade, not the buffer. */
    val label: String,
) {
    /** Smallest buffer: fastest zap, least tolerant of a jittery source. */
    LOW("Fastest start"),

    /** The default trade-off. */
    BALANCED("Balanced"),

    /** Largest buffer: slowest to start, rides out a bad connection. */
    GENEROUS("Smoothest playback"),
}

/**
 * Everything an engine needs to open a stream. Flat, owned, and engine-neutral: the same value
 * loads on any engine, which is what lets "Try other player" re-issue the identical request.
 */
data class StreamRequest(
    /** The stream URL, already validated by the core's locator type. */
    val locator: String,
    /** Per-channel/source header overrides. */
    val headers: ImmutableList<StreamHeader> = persistentListOf(),
    /** Per-channel/source user-agent override; `null` means the engine's own default. */
    val userAgent: String? = null,
    /** The latency/resilience trade-off to apply. */
    val buffering: BufferingProfile = BufferingProfile.BALANCED,
)

/** How video fills the screen. Cycled by the playback UI; every engine honours the same set. */
enum class AspectMode(
    /** The couch-legible label (PRD §8.6 voice). */
    val label: String,
) {
    /** Preserve aspect, letterbox to fit. */
    FIT("Fit"),

    /** Preserve aspect, crop to fill. */
    FILL("Fill"),

    /** Ignore aspect, stretch to the screen. */
    STRETCH("Stretch"),
    ;

    /** The next mode in the cycle, so the UI's aspect button is one call with no index maths. */
    val next: AspectMode
        get() =
            when (this) {
                FIT -> FILL
                FILL -> STRETCH
                STRETCH -> FIT
            }
}
