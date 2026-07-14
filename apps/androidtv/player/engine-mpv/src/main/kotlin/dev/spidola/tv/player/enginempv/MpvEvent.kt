// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

/**
 * One event from mpv's queue, as the JNI bridge hands it over.
 *
 * **This class's shape is a binary contract with `mpv_jni.c`.** The native side looks it up
 * by name (`FindClass("dev/spidola/tv/player/enginempv/MpvEvent")`) and constructs it
 * through a `jmethodID` resolved against the exact descriptor
 * `(IIILjava/lang/String;Ljava/lang/String;)V`. Renaming this class, moving it, reordering
 * these parameters, or changing a type breaks the lookup at runtime — inside `JNI_OnLoad`,
 * long before any Kotlin here runs, and with no compiler warning. `consumer-rules.pro`
 * keeps R8 from doing the same thing by accident.
 *
 * Deliberately flat and stringly-typed: the alternative is walking mpv's tagged-union node
 * tree in C, and the properties this engine observes are all registered as
 * `MPV_FORMAT_STRING` or `MPV_FORMAT_NONE` precisely so that walk is never needed.
 */
internal data class MpvEvent(
    /** `MPV_EVENT_*`. See [MpvClient.EventId] for the ones this engine acts on. */
    val eventId: Int,
    /** `MPV_END_FILE_REASON_*` for an end-file event; `-1` otherwise. */
    val endFileReason: Int,
    /** mpv's error code for an end-file event whose reason is error; `0` otherwise. */
    val endFileError: Int,
    /** Property name for a property change, or log prefix for a log message. */
    val name: String?,
    /** Property value for a property change, or message text for a log message. */
    val value: String?,
)
