// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import android.view.Surface
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Owns one `mpv_handle` and everything that must happen in order around it.
 *
 * This class exists so that [MpvEngine] never touches a raw native handle. Every rule that
 * makes libmpv safe to use from the JVM is enforced here, in one place, because they are
 * rules about *this handle's lifetime* and splitting them across the engine would mean the
 * engine could get them wrong per call site:
 *
 *  - **Destroy happens once, and only once the pump is dead.** [release] flips [closed]
 *    first, wakes the event thread, **joins** it, and calls `mpv_terminate_destroy` only once
 *    that join has actually ended the thread — the join is bounded, so it can return with the
 *    pump still running. A pump that outlives its join keeps the handle alive rather than
 *    losing it: the wait is handed to [destroyWhenPumpDies], which has no deadline and frees
 *    the handle once the thread is gone. Destroying while `mpv_wait_event` is blocked in that
 *    thread is a use-after-free, and it is the single most likely way this engine crashes —
 *    the zap path releases and rebuilds an engine per channel flip, so a one-in-a-hundred
 *    race is a nightly crash.
 *  - **No call touches a destroyed handle.** [closed] gates every entry point, so a
 *    surface teardown or a late command arriving after release is a no-op instead of a
 *    jump through freed memory.
 *  - **release is idempotent.** The contract requires it: the shell calls it from
 *    `DisposableEffect.onDispose` and again on the terminal-state path, and neither knows
 *    whether the other ran.
 */
internal class MpvClient private constructor() {
    /**
     * The `mpv_handle*`. A `var` only because Kotlin binds `external` members as instance
     * methods, so [create] needs an instance in hand before it can call `mpv_create` — it is
     * assigned exactly once, there, and never again.
     */
    private var handle: Long = 0

    private val closed = AtomicBoolean(false)
    private var pump: Thread? = null

    /** Whether [release] has run. Callers use it to stop emitting after teardown. */
    val isClosed: Boolean get() = closed.get()

    /** `MPV_EVENT_*` values this engine acts on, from `client.h`. */
    object EventId {
        const val NONE = 0
        const val SHUTDOWN = 1
        const val LOG_MESSAGE = 2
        const val START_FILE = 6
        const val END_FILE = 7
        const val FILE_LOADED = 8
        const val PLAYBACK_RESTART = 21
        const val PROPERTY_CHANGE = 22
    }

    /** `mpv_format` values, from `client.h`. */
    object Format {
        const val NONE = 0
        const val STRING = 1
    }

    /**
     * Starts the event pump on a dedicated thread.
     *
     * `mpv_wait_event` blocks, so it cannot share a dispatcher thread with anything else —
     * a coroutine on `Dispatchers.IO` would hold an IO thread hostage for the engine's whole
     * life. A plain thread we own is both cheaper and the only way to *join* it, which is how
     * [release] keeps teardown synchronous in the ordinary case.
     *
     * [onEvent] is invoked on that thread; the caller is responsible for hopping to wherever
     * its state lives.
     */
    fun startEventPump(onEvent: (MpvEvent) -> Unit) {
        check(pump == null) { "event pump already started" }
        pump =
            Thread {
                while (!closed.get()) {
                    // The timeout is the pump's only liveness guarantee if a wakeup is ever
                    // missed; it is not how release() ends the loop (that is wakeup + the
                    // closed flag), so it can be long enough to cost nothing while idle.
                    val event = nativeWaitEvent(handle, EVENT_TIMEOUT_SECONDS) ?: continue
                    if (event.eventId == EventId.SHUTDOWN) break
                    if (closed.get()) break
                    onEvent(event)
                }
            }.apply {
                name = "spidola-mpv-events"
                // Not a daemon: release() joins it, and a daemon thread would let the process
                // exit mid-teardown with mpv still holding the surface.
                isDaemon = false
                start()
            }
    }

    /** Initialises the core. Call once, after the pre-init options are set. */
    fun initialize(): Int = ifOpen { nativeInitialize(handle) }

    fun setOption(
        name: String,
        value: String,
    ): Int = ifOpen { nativeSetOptionString(handle, name, value) }

    fun setProperty(
        name: String,
        value: String,
    ): Int = ifOpen { nativeSetPropertyString(handle, name, value) }

    fun getProperty(name: String): String? = if (closed.get()) null else nativeGetPropertyString(handle, name)

    fun command(vararg args: String): Int = ifOpen { nativeCommand(handle, arrayOf(*args)) }

    fun observeProperty(
        name: String,
        format: Int,
    ): Int = ifOpen { nativeObserveProperty(handle, name, format) }

    fun requestLogMessages(level: String): Int = ifOpen { nativeRequestLogMessages(handle, level) }

    /**
     * Hands mpv the window to render into.
     *
     * Must be called while mpv's video output is down — `wid` is an option, not a property,
     * and mpv only reads it when the VO initialises. [MpvEngine] therefore attaches first and
     * turns `vo` on afterwards.
     */
    fun attachSurface(surface: Surface): Int = ifOpen { nativeAttachSurface(handle, surface) }

    /**
     * Releases the window.
     *
     * **The caller must have already driven `vo=null` and let it complete.** mpv's render
     * thread dereferences the Surface global ref through `ANativeWindow_fromSurface`; dropping
     * that ref while the VO is live is a use-after-free. `mpv_set_property_string("vo", ...)`
     * is synchronous, so returning from it is the guarantee this needs.
     *
     * Safe after [release]: the surface can outlive the engine, and the native side tolerates
     * a zero handle precisely for that case.
     */
    fun detachSurface() {
        nativeDetachSurface(if (closed.get()) 0L else handle)
    }

    /**
     * Tears down mpv. Idempotent.
     *
     * Safe from any thread, but do not call it from the event pump: the join below would then
     * wait on the caller's own thread, burning [PUMP_JOIN_TIMEOUT_MS] before handing that same
     * thread's death to [destroyWhenPumpDies] to wait for all over again.
     *
     * The ordering below is the whole reason this method is not three lines at the call site.
     */
    fun release() {
        // compareAndSet, not a get/set pair: two DisposableEffect disposals racing must not
        // both reach mpv_terminate_destroy.
        if (!closed.compareAndSet(false, true)) return

        // Unblock a wait in progress. Safe on a live handle from another thread — it is the
        // one mpv call documented as such.
        nativeWakeup(handle)

        // Join before destroy. Everything else here is ordinary cleanup; this and the branch
        // below are the lines that prevent the crash.
        val thread = pump
        pump = null
        if (thread != null) {
            try {
                thread.join(PUMP_JOIN_TIMEOUT_MS)
            } catch (interrupted: InterruptedException) {
                // Restore the flag rather than swallow it: this runs on a caller's thread,
                // and eating an interrupt would break that thread's own cancellation.
                Thread.currentThread().interrupt()
            }
            // join(timeout) returns the same way whether the thread ended or the timeout
            // did, and an interrupt cuts it short earlier still; isAlive is the only thing
            // that tells those apart.
            if (thread.isAlive) {
                // Destroying under a live pump is precisely the use-after-free this class is
                // shaped to prevent, so this thread does not destroy. It hands the wait to one
                // that can afford to block instead of dropping the handle on the floor.
                MpvLog.pumpOutlivedJoin(PUMP_JOIN_TIMEOUT_MS)
                destroyWhenPumpDies(thread)
                return
            }
        }

        nativeDestroy(handle)
    }

    /**
     * Waits for a pump that outlived [PUMP_JOIN_TIMEOUT_MS] — this time with no deadline — and
     * destroys the handle it was still holding.
     *
     * The join is the whole mechanism: it returns when the thread is *dead*, however it died —
     * a normal exit, an uncaught exception out of `onEvent`, anything. Death is the property
     * that matters, because what makes the destroy safe is that no thread is inside
     * `mpv_wait_event` any more, and a dead thread is not inside anything.
     *
     * A daemon, because the wait is unbounded: if mpv never returns from that wait the thread
     * parks forever and the handle stays leaked — no worse than not deferring at all — but a
     * non-daemon thread would additionally hold the process open waiting for it.
     */
    private fun destroyWhenPumpDies(thread: Thread) {
        Thread {
            thread.join()
            nativeDestroy(handle)
        }.apply {
            name = "spidola-mpv-teardown"
            isDaemon = true
            start()
        }
    }

    private inline fun ifOpen(block: () -> Int): Int = if (closed.get()) MpvErrorMapping.Code.GENERIC else block()

    // The native side resolves these by name against this exact class and package. See
    // MpvEvent's doc and consumer-rules.pro.
    private external fun nativeInitialize(handle: Long): Int

    private external fun nativeDestroy(handle: Long)

    private external fun nativeSetOptionString(
        handle: Long,
        name: String,
        value: String,
    ): Int

    private external fun nativeSetPropertyString(
        handle: Long,
        name: String,
        value: String,
    ): Int

    private external fun nativeGetPropertyString(
        handle: Long,
        name: String,
    ): String?

    private external fun nativeCommand(
        handle: Long,
        args: Array<String>,
    ): Int

    private external fun nativeObserveProperty(
        handle: Long,
        name: String,
        format: Int,
    ): Int

    private external fun nativeRequestLogMessages(
        handle: Long,
        level: String,
    ): Int

    private external fun nativeWakeup(handle: Long)

    private external fun nativeWaitEvent(
        handle: Long,
        timeout: Double,
    ): MpvEvent?

    private external fun nativeAttachSurface(
        handle: Long,
        surface: Surface,
    ): Int

    private external fun nativeDetachSurface(handle: Long)

    private external fun nativeCreate(): Long

    companion object {
        private const val EVENT_TIMEOUT_SECONDS = 1.0

        /**
         * How long [release] waits for the pump *on the caller's thread*. Bounded rather than
         * indefinite: if mpv ever wedges inside `mpv_wait_event`, a permanent block here would
         * freeze the UI thread that called `release()` and take the whole app down. An ANR on
         * every zap is worse than a late teardown, and destroying the handle out from under the
         * still-blocked pump is worse than both — so when this expires the wait moves to
         * [destroyWhenPumpDies] rather than the handle being dropped. Only a pump that never
         * dies at all — still blocked in that wait, forever — costs a handle.
         */
        private const val PUMP_JOIN_TIMEOUT_MS = 2_000L

        private val loaded = AtomicBoolean(false)
        private val loadFailure = AtomicBoolean(false)

        /**
         * Loads `libmpv.so` once per process.
         *
         * Returns false rather than throwing when the library is absent, so a build without
         * the native artifact degrades to "this engine is unavailable" — which the engine
         * reports as an honest [dev.spidola.tv.core.playercontract.EngineError] — instead of
         * an `UnsatisfiedLinkError` escaping into a composable.
         */
        private fun ensureLoaded(): Boolean {
            if (loaded.get()) return true
            if (loadFailure.get()) return false
            return try {
                System.loadLibrary(CXX_RUNTIME_LIBRARY)
                System.loadLibrary(NATIVE_LIBRARY)
                loaded.set(true)
                true
            } catch (error: UnsatisfiedLinkError) {
                MpvLog.nativeLibraryMissing(error)
                loadFailure.set(true)
                false
            }
        }

        /** The JNI shim; it links libmpv.so, so loading it loads both. */
        private const val NATIVE_LIBRARY = "spidola_mpv"
        private const val CXX_RUNTIME_LIBRARY = "c++_shared"

        /**
         * Creates an uninitialised mpv instance, or `null` when the native library is missing
         * or mpv cannot allocate.
         *
         * Nothing here opens a stream or touches the network: the contract requires
         * construction to be free of I/O because the zap path builds an engine per channel
         * flip. `mpv_create` allocates a context; `load()` is where work starts.
         */
        fun create(): MpvClient? {
            if (!ensureLoaded()) return null
            val client = MpvClient()
            client.handle = client.nativeCreate()
            if (client.handle == 0L) return null
            return client
        }
    }
}
