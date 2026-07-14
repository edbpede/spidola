// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import dev.spidola.tv.core.playercontract.AspectMode
import dev.spidola.tv.core.playercontract.BufferingProfile
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.EngineId
import dev.spidola.tv.core.playercontract.PlaybackEngine
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.StreamRequest
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackSelection
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The libmpv engine — mpv-class codec breadth, the Android fallback (TECH_SPEC §8).
 *
 * Construction is free of I/O and touches no mpv: the zap path destroys and rebuilds an
 * engine per channel flip, so everything expensive waits for [load].
 *
 * Threading: the contract makes engines main-thread-affine, and mpv's event pump is not.
 * Events arrive on [MpvClient]'s pump thread and are published straight into
 * [MutableStateFlow]s, which are safe to write from any thread. That is deliberately all the
 * concurrency machinery here — no dispatcher, no scope this engine would then have to own
 * and cancel, and no `runBlocking`.
 */
class MpvEngine : PlaybackEngine {
    override val id: EngineId = EngineId.MPV

    private val _state = MutableStateFlow<PlaybackState>(PlaybackState.Idle)
    override val state: StateFlow<PlaybackState> = _state.asStateFlow()

    private val _tracks = MutableStateFlow(TrackSelection())
    override val tracks: StateFlow<TrackSelection> = _tracks.asStateFlow()

    private val _isSeekable = MutableStateFlow(false)
    override val isSeekable: StateFlow<Boolean> = _isSeekable.asStateFlow()

    private val released = AtomicBoolean(false)

    @Volatile
    private var client: MpvClient? = null

    /**
     * The last few lines mpv logged, kept only long enough to classify a failure.
     * [MpvErrorMapping] needs them because mpv's error codes cannot tell 401 from a DNS
     * failure. Bounded, and redacted on the way in.
     */
    private val recentDiagnostics = MpvDiagnosticBuffer()

    @Volatile
    private var flags = MpvStateDerivation.Flags()

    /**
     * Serialises surface attach/detach against each other and against teardown. The
     * composable disposes on the main thread while the pump runs elsewhere, and mpv's VO
     * teardown must not interleave with a new attach.
     */
    private val surfaceLock = Any()
    private var attachedSurface: android.view.Surface? = null

    @Composable
    override fun Surface(modifier: Modifier) {
        // SurfaceView, not TextureView. On TV this is not a close call: a SurfaceView gets a
        // dedicated hardware overlay plane, so video bypasses view compositing entirely.
        // TextureView routes every frame through GPU composition, which costs power and
        // rules out HDR passthrough — on exactly the low-end Chromecast-class device PRD §9
        // sets as the baseline.
        Box(modifier) {
            AndroidView(
                factory = { context ->
                    SurfaceView(context).apply {
                        holder.addCallback(
                            object : SurfaceHolder.Callback {
                                override fun surfaceCreated(holder: SurfaceHolder) {
                                    onSurfaceAvailable(holder.surface)
                                }

                                override fun surfaceChanged(
                                    holder: SurfaceHolder,
                                    format: Int,
                                    width: Int,
                                    height: Int,
                                ) {
                                    // mpv reads the window size from the ANativeWindow itself
                                    // (android_common.c:84-96). Telling it again would only
                                    // race that read.
                                }

                                override fun surfaceDestroyed(holder: SurfaceHolder) {
                                    onSurfaceLost()
                                }
                            },
                        )
                    }
                },
            )
        }
    }

    override fun load(request: StreamRequest) {
        if (released.get()) return
        check(client == null) { "load() called twice; engines are single-use — rebuild instead" }

        MpvLog.loading(request)
        publish(PlaybackState.Loading)

        val mpv = MpvClient.create()
        if (mpv == null) {
            // No libmpv in this build. An honest terminal failure beats an
            // UnsatisfiedLinkError escaping into a composable.
            publish(PlaybackState.Failed(EngineError.Unknown("libmpv is not available in this build")))
            return
        }
        client = mpv

        applyPreInitOptions(mpv, request)

        val initRc = mpv.initialize()
        if (initRc < 0) {
            publish(PlaybackState.Failed(MpvErrorMapping.engineErrorFor(initRc, "mpv_initialize failed")))
            return
        }

        // Warnings and worse only. Not merely to keep logcat quiet: mpv's info level echoes
        // the options it resolved, which is where a token-bearing header would be printed.
        // MpvLogRedaction is the backstop, not the first line of defence.
        mpv.requestLogMessages("warn")
        observeDefaults(mpv)

        // The composable may have produced a surface before load() ran — there is no ordering
        // guarantee between them, so whichever arrives second does the attach.
        synchronized(surfaceLock) {
            attachedSurface?.let { attach(mpv, it) }
        }

        mpv.startEventPump(::onEvent)

        // Everything above is configuration. This is the line that starts work.
        val loadRc = mpv.command("loadfile", request.locator)
        if (loadRc < 0) {
            publish(PlaybackState.Failed(MpvErrorMapping.engineErrorFor(loadRc, recentDiagnostics.snapshot())))
        }
    }

    override fun play() {
        client?.setProperty("pause", "no")
    }

    override fun pause() {
        client?.setProperty("pause", "yes")
    }

    override fun seekTo(seconds: Double) {
        // A no-op when not seekable, per the contract, so the caller need not guard.
        if (!_isSeekable.value) return
        client?.command("seek", seconds.toString(), "absolute")
    }

    override fun select(track: TrackId) {
        val selection = MpvTrackMapping.selectionFor(track) ?: return
        client?.setProperty(selection.first, selection.second)
    }

    override fun clearSubtitle() {
        // "no" is mpv's off value for a track property. Distinct from selecting a track,
        // because "no subtitle" is not a track — which is why the contract has this method.
        client?.setProperty("sid", "no")
    }

    override fun setAspect(mode: AspectMode) {
        val mpv = client ?: return
        when (mode) {
            // keepaspect + panscan=0: letterbox, whole frame visible.
            AspectMode.FIT -> {
                mpv.setProperty("keepaspect", "yes")
                mpv.setProperty("panscan", "0")
            }
            // panscan=1 zooms until the frame fills the screen, cropping the overflow.
            AspectMode.FILL -> {
                mpv.setProperty("keepaspect", "yes")
                mpv.setProperty("panscan", "1")
            }
            // keepaspect=no stretches to the window, ignoring the source ratio.
            AspectMode.STRETCH -> {
                mpv.setProperty("keepaspect", "no")
                mpv.setProperty("panscan", "0")
            }
        }
    }

    override fun release() {
        if (!released.compareAndSet(false, true)) return
        val mpv = client ?: return

        // Surface first, then mpv: detachSurface's contract is that the VO is already down.
        synchronized(surfaceLock) {
            if (attachedSurface != null) {
                mpv.setProperty("vo", "null")
                mpv.detachSurface()
                attachedSurface = null
            }
        }

        // Joins the pump, then destroys the handle.
        mpv.release()
        client = null
    }

    // region Surface ownership

    private fun onSurfaceAvailable(surface: android.view.Surface) {
        synchronized(surfaceLock) {
            attachedSurface = surface
            // load() may not have run yet; it re-attaches from attachedSurface if so.
            client?.let { attach(it, surface) }
        }
    }

    private fun onSurfaceLost() {
        synchronized(surfaceLock) {
            val mpv = client
            if (mpv != null && attachedSurface != null) {
                // The ordering that prevents the use-after-free. mpv's render thread holds an
                // ANativeWindow derived from this Surface; `vo` is a property, so setting it
                // is synchronous, and when it returns the VO is down and the window released.
                // Only then may the global ref go.
                //
                // surfaceDestroyed must not return until nothing will touch the surface
                // again — the system frees it immediately after. So blocking here is the
                // contract, not an oversight.
                mpv.setProperty("vo", "null")
                mpv.detachSurface()
            }
            attachedSurface = null
        }
    }

    private fun attach(
        mpv: MpvClient,
        surface: android.view.Surface,
    ) {
        val rc = mpv.attachSurface(surface)
        if (rc < 0) {
            MpvLog.nativeCallFailed("attachSurface", rc)
            return
        }
        // vo comes up only after wid is set: mpv reads wid when the VO initialises, so the
        // reverse order hands it a null window.
        mpv.setProperty("vo", "gpu")
    }

    // endregion

    private fun applyPreInitOptions(
        mpv: MpvClient,
        request: StreamRequest,
    ) {
        // vo starts null and is switched on when a surface arrives; otherwise mpv initialises
        // video output against a window that does not exist yet.
        mpv.setOption("vo", "null")
        mpv.setOption("gpu-context", "android")
        mpv.setOption("ao", "audiotrack,opensles")
        // This is an embedded library: mpv's own config and scripts must never load, or a
        // stray config on a rooted device would change playback unreproducibly.
        mpv.setOption("config", "no")
        mpv.setOption("osc", "no")
        mpv.setOption("input-default-bindings", "no")
        // MediaCodec hardware decode, falling back to software. `-copy` reads frames back for
        // the GL renderer: it costs memory bandwidth, but direct rendering needs a surface
        // mpv owns outright, which is incompatible with handing it ours.
        mpv.setOption("hwdec", "mediacodec-copy,no")

        applyBuffering(mpv, request.buffering)
        applyHeaders(mpv, request)
    }

    private fun applyBuffering(
        mpv: MpvClient,
        profile: BufferingProfile,
    ) {
        // Honest accounting: these are starting points, not measured optima. The trade is
        // real — cache-secs is roughly how long a stall can last before the viewer notices,
        // and roughly what LOW gives back in click-to-first-frame. They bracket PRD §9's
        // two-second first-frame bar by reasoning, not by measurement, and want profiling on
        // the low-end baseline before anyone calls them tuned.
        val cacheSecs: String
        val readaheadSecs: String
        when (profile) {
            BufferingProfile.LOW -> {
                cacheSecs = "2"
                readaheadSecs = "1"
            }
            BufferingProfile.BALANCED -> {
                cacheSecs = "10"
                readaheadSecs = "5"
            }
            BufferingProfile.GENEROUS -> {
                cacheSecs = "30"
                readaheadSecs = "20"
            }
        }
        mpv.setOption("cache", "yes")
        mpv.setOption("cache-secs", cacheSecs)
        mpv.setOption("demuxer-readahead-secs", readaheadSecs)
    }

    private fun applyHeaders(
        mpv: MpvClient,
        request: StreamRequest,
    ) {
        request.userAgent?.let { mpv.setOption("user-agent", it) }

        // -append, one header per call, rather than a comma-joined list.
        // `http-header-fields` is a comma-separated list option, so a value containing a
        // comma — as a Cookie routinely does — would silently split into two malformed
        // headers. Appending sidesteps the escaping question entirely.
        request.headers.forEach { header ->
            mpv.setOption("http-header-fields-append", "${header.name}: ${header.value}")
        }
    }

    private fun observeDefaults(mpv: MpvClient) {
        // STRING because mpv renders flags as "yes"/"no" in that format, which keeps the JNI
        // payload a plain string and the C side free of node-tree walking.
        listOf("pause", "paused-for-cache", "core-idle", "seekable").forEach {
            mpv.observeProperty(it, MpvClient.Format.STRING)
        }
        // NONE: notification with no payload. track-list is a node, and re-reading its flat
        // sub-properties on change is cheaper than marshalling the tree across JNI.
        mpv.observeProperty("track-list", MpvClient.Format.NONE)
    }

    private fun onEvent(event: MpvEvent) {
        if (released.get()) return
        val mpv = client ?: return

        when (event.eventId) {
            MpvClient.EventId.LOG_MESSAGE -> {
                val text = event.value ?: return
                MpvLog.mpvSaid(event.name, text)
                recentDiagnostics.add(MpvLogRedaction.redact(text))
            }

            MpvClient.EventId.START_FILE -> {
                flags = MpvStateDerivation.Flags()
                publish(MpvStateDerivation.stateFor(flags))
            }

            MpvClient.EventId.FILE_LOADED -> {
                flags = flags.copy(fileLoaded = true)
                refreshTracks(mpv)
                refreshFlags(mpv)
            }

            MpvClient.EventId.PLAYBACK_RESTART -> refreshFlags(mpv)

            MpvClient.EventId.PROPERTY_CHANGE -> onPropertyChange(mpv, event)

            MpvClient.EventId.END_FILE -> onEndFile(event)

            else -> Unit
        }
    }

    private fun onEndFile(event: MpvEvent) {
        val error =
            MpvErrorMapping.endFileError(
                reason = event.endFileReason,
                errorCode = event.endFileError,
                diagnostic = recentDiagnostics.snapshot(),
            )
        when {
            error != null -> publish(PlaybackState.Failed(error))
            event.endFileReason == MpvErrorMapping.EndFileReason.EOF -> publish(PlaybackState.Ended)
            // STOP/QUIT/REDIRECT: our own teardown, or mpv following a redirect. Neither is
            // something the viewer should see a state change for.
            else -> Unit
        }
    }

    private fun onPropertyChange(
        mpv: MpvClient,
        event: MpvEvent,
    ) {
        when (event.name) {
            "seekable" -> _isSeekable.value = MpvStateDerivation.flagOf(event.value)
            "track-list" -> refreshTracks(mpv)
            "pause" -> updateFlags(flags.copy(pause = MpvStateDerivation.flagOf(event.value)))
            "paused-for-cache" -> updateFlags(flags.copy(pausedForCache = MpvStateDerivation.flagOf(event.value)))
            "core-idle" -> updateFlags(flags.copy(coreIdle = MpvStateDerivation.flagOf(event.value)))
            else -> Unit
        }
    }

    private fun updateFlags(next: MpvStateDerivation.Flags) {
        flags = next
        publish(MpvStateDerivation.stateFor(next))
    }

    private fun refreshFlags(mpv: MpvClient) {
        updateFlags(
            flags.copy(
                pause = MpvStateDerivation.flagOf(mpv.getProperty("pause")),
                pausedForCache = MpvStateDerivation.flagOf(mpv.getProperty("paused-for-cache")),
                coreIdle = MpvStateDerivation.flagOf(mpv.getProperty("core-idle")),
            ),
        )
        _isSeekable.value = MpvStateDerivation.flagOf(mpv.getProperty("seekable"))
    }

    private fun refreshTracks(mpv: MpvClient) {
        val count = mpv.getProperty("track-list/count")?.toIntOrNull() ?: return
        val rows =
            (0 until count).mapNotNull { index ->
                val id = mpv.getProperty("track-list/$index/id")?.toIntOrNull() ?: return@mapNotNull null
                val type = mpv.getProperty("track-list/$index/type") ?: return@mapNotNull null
                MpvTrackMapping.RawTrack(
                    id = id,
                    type = type,
                    title = mpv.getProperty("track-list/$index/title"),
                    lang = mpv.getProperty("track-list/$index/lang"),
                    selected = MpvStateDerivation.flagOf(mpv.getProperty("track-list/$index/selected")),
                )
            }
        _tracks.value = MpvTrackMapping.toTrackSelection(rows)
    }

    private fun publish(next: PlaybackState) {
        val previous = _state.value
        if (previous == next) return
        // Failed latches. The contract says a failed engine is spent and the shell disposes
        // it, so a late property change must not resurrect it into Playing.
        //
        // Only Failed — not Ended. The contract's own `isTerminal` also counts Ended, which is
        // right for the shell (it disposes on both), but a live stream can emit END_FILE with
        // EOF and be reloaded by mpv, and latching Ended here would freeze an engine whose
        // source recovered by itself.
        if (previous is PlaybackState.Failed) return
        MpvLog.transition(previous, next)
        _state.value = next
    }
}
