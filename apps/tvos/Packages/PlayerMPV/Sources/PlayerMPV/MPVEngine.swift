// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import Libmpv
import OSLog
import PlayerContract
import SwiftUI

/// The MPVKit engine — the tvOS default (TECH_SPEC §8, §14: AVPlayer's format range is too narrow
/// to be the default for IPTV reality, so mpv's codec breadth takes the position).
///
/// Construction is deliberately free of I/O — no `mpv_create`, no session, no thread. The zap path
/// destroys and rebuilds engines on every channel change and its budget is an acceptance test
/// (PRD §9), so everything expensive waits for `load`.
@MainActor
public final class MPVEngine: PlaybackEngine {
  public nonisolated let id = EngineID.mpv
  public nonisolated let states: AsyncStream<PlaybackState>

  public private(set) var tracks = TrackSelection()
  public private(set) var isSeekable = false

  private let statesContinuation: AsyncStream<PlaybackState>.Continuation

  /// The render target. Built at init because it is cheap (no drawable is allocated until mpv
  /// renders) and because mpv needs its address before `mpv_initialize` — and held here, strongly,
  /// because MPVKit's MoltenVK context bridges `wid` back to a `CAMetalLayer` **unretained**. If
  /// this reference dies while the core lives, mpv renders into freed memory.
  ///
  /// Holding it here is necessary but not sufficient: the core outlives this engine by design, on
  /// `MPVCoreDisposal`'s thread, so the layer is handed to that thread as well and released only
  /// once `mpv_destroy` has returned.
  private let metalLayer = MPVMetalLayer()

  private let audioSession = MPVAudioSession()
  private let nowPlaying = MPVNowPlaying()

  /// The core handle. Owned exclusively by the main actor; `nil` once `stop()` has handed it off.
  private var core: MPVHandle?
  private var tasks: [Task<Void, Never>] = []

  private var facts = MPVPlaybackFacts()
  private var currentState: PlaybackState = .idle
  /// The last failure mpv's log implied. mpv reports *why* a load failed in its log stream and
  /// *that* it failed in a later `MPV_EVENT_END_FILE`, so the classification has to be carried
  /// across the gap; see `MPVErrorMapping.logHint`.
  private var lastLogHint: EngineError?
  private var isStopped = false
  /// Whether the viewer was playing when the system interrupted us, so `.ended(shouldResume:)`
  /// resumes only what it actually paused.
  private var wasPlayingBeforeInterruption = false
  private var mediaTitle: String?

  public init() {
    let (stream, continuation) = AsyncStream<PlaybackState>.makeStream(bufferingPolicy: .unbounded)
    self.states = stream
    self.statesContinuation = continuation
    continuation.yield(.idle)
  }

  public func makeSurface() -> AnyView {
    AnyView(MPVMetalSurface(metalLayer: metalLayer))
  }

  // MARK: - Loading

  public func load(_ request: StreamRequest) {
    guard !isStopped, core == nil else { return }

    Logger.mpv.info(
      """
      loading \(MPVRedaction.locatorSummary(request.locator), privacy: .public) \
      headers=[\(MPVRedaction.headerNames(request.headers), privacy: .public)] \
      user-agent=\(MPVRedaction.userAgentPresence(request.userAgent), privacy: .public) \
      buffering=\(request.buffering.rawValue, privacy: .public)
      """)

    emit(.loading)
    do {
      let core = try makeCore(for: request)
      self.core = core
      try startEventLoop(on: core)
      startSystemIntegration()
      try core.command(["loadfile", request.locator, "replace"])
    } catch let error as MPVCallError {
      Logger.mpv.error(
        "load failed during \(error.operation, privacy: .public): \(error.code, privacy: .public)")
      emit(.failed(MPVErrorMapping.engineError(mpvError: error.code, logHint: lastLogHint)))
    } catch {
      emit(.failed(.unknown(detail: "unexpected engine error")))
    }
  }

  /// Builds and initialises the core.
  ///
  /// Everything here happens **before** `mpv_initialize` because mpv only reads these at
  /// initialisation: `wid` and the video-output stack cannot be changed afterwards, and setting
  /// them late silently leaves mpv on its own default output with nowhere to draw.
  private func makeCore(for request: StreamRequest) throws -> MPVHandle {
    let core = try MPVHandle.create()
    do {
      try configure(core, for: request)
    } catch {
      // The handle exists but nothing owns it yet: `self.core` is only assigned once this returns,
      // so an early throw here would strand the core with no path to `stop()` and leak a decoder
      // on every failed load — which on a bad source is every zap.
      MPVCoreDisposal.dispose(core.raw, keepingAlive: metalLayer)
      throw error
    }
    return core
  }

  private func configure(_ core: MPVHandle, for request: StreamRequest) throws {
    // The render path. See `MPVMetalLayer` for why this is `wid` + MoltenVK rather than libmpv's
    // render API — in short, that API has no Metal type and tvOS has no future in OpenGL.
    try core.setWindowID(Unmanaged.passUnretained(metalLayer).toOpaque())
    try core.setOption("vo", "gpu-next")
    try core.setOption("gpu-api", "vulkan")
    try core.setOption("gpu-context", "moltenvk")

    // VideoToolbox is the whole reason mpv is viable on this hardware — an Apple TV cannot software
    // decode 4K HEVC. The simulator has no VideoToolbox backing, so it decodes in software there;
    // without this branch every simulator run fails at the decoder rather than showing a picture.
    #if targetEnvironment(simulator)
      try core.setOption("hwdec", "no")
    #else
      try core.setOption("hwdec", "videotoolbox")
    #endif

    // mpv would otherwise take the terminal and stdin, neither of which exists in an app, and its
    // config/OSD would fight the shell's own UI (PRD §6.3: the shell owns the overlays).
    try core.setOption("config", "no")
    try core.setOption("terminal", "no")
    try core.setOption("input-default-bindings", "no")
    try core.setOption("input-vo-keyboard", "no")
    try core.setOption("osd-level", "0")
    // Rotation metadata is the source's business, not ours to re-apply on a TV.
    try core.setOption("video-rotate", "no")

    // Deliberately absent: `osc`. mpv registers it only under `#if HAVE_LUA`, and MPVKit builds
    // Lua for macOS alone (`-Dlua=luajit`) while every other platform, tvOS included, gets
    // `-Dlua=disabled`. Setting it here would throw MPV_ERROR_OPTION_NOT_FOUND and fail every
    // load on device while passing on a Mac. There is no on-screen controller to disable without
    // Lua, so the option has nothing to do here anyway.

    try apply(request: request, to: core)
    for option in MPVOptions.aspectOptions(for: .fit) {
      try core.setOption(option)
    }

    try core.initialize()
  }

  private func apply(request: StreamRequest, to core: MPVHandle) throws {
    for option in MPVOptions.cacheOptions(for: request.buffering) {
      try core.setOption(option)
    }
    // Verbatim through a node array — mpv's option parser would split a header value on its commas.
    try core.setStringList("http-header-fields", MPVOptions.headerFields(request.headers))
    if let userAgent = request.userAgent {
      try core.setOption("user-agent", userAgent)
    }
  }

  /// The properties whose changes drive the state machine.
  ///
  /// `track-list` is observed with `MPV_FORMAT_NONE` — a bare "it changed" notification. The value
  /// is then fetched as JSON, because reading a node tree through the event payload would mean
  /// hand-rolled pointer traversal for data `MPVTrackList` can parse from a string.
  private static let observedProperties: [MPVObservedProperty] = [
    MPVObservedProperty(name: "core-idle", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "paused-for-cache", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "eof-reached", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "pause", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "idle-active", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "seekable", format: MPV_FORMAT_FLAG),
    MPVObservedProperty(name: "duration", format: MPV_FORMAT_DOUBLE),
    MPVObservedProperty(name: "time-pos", format: MPV_FORMAT_DOUBLE),
    MPVObservedProperty(name: "media-title", format: MPV_FORMAT_STRING),
    MPVObservedProperty(name: "track-list", format: MPV_FORMAT_NONE),
  ]

  private func startEventLoop(on core: MPVHandle) throws {
    let clientHandle = try core.createWeakClientHandle(name: "spidola_events")
    let events = MPVEventLoop.start(
      clientHandle: clientHandle, observedProperties: Self.observedProperties)

    // The stream is hoisted out of the closure deliberately: `AsyncStream` is `Sendable`, so the
    // task captures only it and a weak `self`, never the engine's non-`Sendable` internals.
    tasks.append(
      Task { [weak self] in
        for await event in events {
          guard let self else { return }
          self.handle(event)
        }
      })
  }

  private func startSystemIntegration() {
    audioSession.activate()
    nowPlaying.activate()

    let interruptions = audioSession.interruptions
    let commands = nowPlaying.commands

    tasks.append(
      Task { [weak self] in
        for await interruption in interruptions {
          guard let self else { return }
          self.handle(interruption)
        }
      })
    tasks.append(
      Task { [weak self] in
        for await command in commands {
          guard let self else { return }
          self.handle(command)
        }
      })
  }

  // MARK: - Transport

  public func play() {
    setPause(false)
  }

  public func pause() {
    setPause(true)
  }

  private func setPause(_ paused: Bool) {
    guard let core else { return }
    do {
      try core.setFlag("pause", paused)
    } catch {
      Logger.mpv.error("failed to set pause=\(paused, privacy: .public)")
    }
  }

  public func seek(toSeconds seconds: Double) {
    // The contract makes this a no-op rather than an error when unseekable, so the caller need not
    // guard — live streams are the common case and every call site would otherwise repeat this.
    guard isSeekable, let core else { return }
    do {
      try core.command(["seek", String(seconds), "absolute"])
    } catch {
      Logger.mpv.error("seek failed")
    }
  }

  public func select(track: TrackID) {
    guard let core, let match = tracks.available.first(where: { $0.id == track }) else { return }
    do {
      // mpv splits track selection across two properties by kind; the contract has one call, so the
      // kind we already recorded picks the property.
      switch match.kind {
      case .audio: try core.setProperty("aid", track.rawValue)
      case .subtitle: try core.setProperty("sid", track.rawValue)
      }
    } catch {
      Logger.mpv.error("track selection failed")
    }
  }

  public func clearSubtitle() {
    guard let core else { return }
    do {
      // "no" is mpv's off value for a track property — distinct from an id, which is why the
      // contract keeps this separate from `select`.
      try core.setProperty("sid", "no")
    } catch {
      Logger.mpv.error("clearing subtitle failed")
    }
  }

  public func setAspect(_ mode: AspectMode) {
    guard let core else { return }
    do {
      for option in MPVOptions.aspectOptions(for: mode) {
        try core.setProperty(option.name, option.value)
      }
    } catch {
      Logger.mpv.error("aspect change failed")
    }
  }

  // MARK: - Teardown

  /// Tears the engine down. Idempotent, per the contract: the shell calls this on dispose and on
  /// the terminal-state path, and neither knows whether the other ran.
  ///
  /// **The ordering, which is the delicate part.** A use-after-free here is a crash on every zap, so
  /// the design removes the race rather than narrowing it. Two handles exist, and each is touched by
  /// exactly one thread for its whole life:
  ///
  /// - `core` — strong, main actor only.
  /// - the event loop's weak client — its own thread only, created and destroyed there.
  ///
  /// Because no handle is shared, no thread can free one another is inside. What remains is waking
  /// the event thread, which sits blocked in `mpv_wait_event`. We never poke it: destroying the last
  /// **strong** handle makes libmpv send `MPV_EVENT_SHUTDOWN` to every **weak** client, which is the
  /// wake-up, delivered by the core itself. The event thread then destroys its own handle and exits.
  ///
  /// `mpv_destroy` on the last strong handle blocks until the weak clients respond and the core
  /// finishes uninitialising, which is why it runs on a disposal thread and not here — blocking the
  /// main actor is what the zap budget cannot afford. This engine sets `core = nil` before handing
  /// the pointer over, so the main actor's last use of it precedes the hand-off by construction.
  ///
  /// The render target crosses with it, retained past `mpv_destroy`. mpv still holds it —
  /// unretained, as a bare `wid` address — for as long as it is uninitialising, while this engine's
  /// own reference to it can die the instant this method returns: on the `deinit` path below, that
  /// is the next thing that happens.
  public func stop() {
    guard !isStopped else { return }
    isStopped = true

    guard let core else {
      statesContinuation.finish()
      return
    }
    self.core = nil

    for task in tasks { task.cancel() }
    tasks.removeAll()
    nowPlaying.deactivate()
    audioSession.deactivate()

    Logger.mpv.info("engine stopping")
    MPVCoreDisposal.dispose(core.raw, keepingAlive: metalLayer)
    statesContinuation.finish()
  }

  /// The contract's backstop for a dropped reference. Without it a live core keeps decoding with
  /// no owner left to stop it — the event task holds its stream, mpv holds the decoder, and the
  /// process-wide audio session and now-playing info stay claimed. `isolated deinit` runs on the
  /// main actor, so this is the same idempotent `stop()` the shell calls, not a second teardown
  /// path.
  isolated deinit {
    stop()
  }

  // MARK: - Events

  private func handle(_ event: MPVEvent) {
    switch event {
    case .shutdown:
      // Expected during our own teardown, and already handled there. Unprompted, it means the core
      // died under us — terminal, and the shell must be told rather than left on a frozen frame.
      guard !isStopped else { return }
      emit(.failed(.unknown(detail: "mpv core shut down unexpectedly")))

    case .fileLoaded:
      facts.fileLoaded = true
      refreshTracks()
      reduce()

    case .endFile(let reason, let mpvError):
      // `nil` means an end-of-file we caused (our own stop, a redirect); saying nothing is correct.
      guard
        let outcome = MPVErrorMapping.endFileOutcome(
          reason: reason, mpvError: mpvError, logHint: lastLogHint)
      else { return }
      emit(outcome)

    case .propertyChanged(let name, let value):
      handleProperty(name: name, value: value)

    case .logMessage(_, let text):
      // Classified and dropped. The text never reaches OSLog — mpv logs full stream URLs, and an
      // Xtream URL carries the account in its path (TECH_SPEC §12). Only the derived class, which
      // is one of six fixed cases, survives this line.
      if let hint = MPVErrorMapping.logHint(from: text) {
        lastLogHint = hint
      }
    }
  }

  private func handleProperty(name: String, value: MPVPropertyValue) {
    switch (name, value) {
    case ("core-idle", .flag(let flag)):
      facts.coreIdle = flag
    case ("paused-for-cache", .flag(let flag)):
      facts.pausedForCache = flag
    case ("eof-reached", .flag(let flag)):
      facts.eofReached = flag
    case ("pause", .flag(let flag)):
      facts.paused = flag
    case ("idle-active", .flag(let flag)):
      // mpv going idle after a file means the playlist ran out. `eof-reached` covers the normal
      // run-out; this catches the case where mpv drops to idle without setting it.
      if flag && facts.fileLoaded { facts.eofReached = true }
    case ("seekable", .flag(let flag)):
      isSeekable = flag
    case ("duration", .double), ("time-pos", .double):
      updateNowPlaying()
      return
    case ("media-title", .string(let title)):
      mediaTitle = title
      updateNowPlaying()
      return
    case ("track-list", _):
      refreshTracks()
      return
    default:
      return
    }
    reduce()
  }

  private func refreshTracks() {
    guard let core, let json = core.string("track-list") else { return }
    tracks = MPVTrackList.parse(json: json)
  }

  /// Re-derives the contract state from the facts and publishes it if it moved.
  ///
  /// De-duplicating matters: mpv fires these properties independently and several arrive per
  /// second, most of which do not change the derived state. Yielding each one would turn the state
  /// stream into a firehose the shell would have to de-duplicate itself.
  private func reduce() {
    emit(MPVStateReducer.state(from: facts))
    updateNowPlaying()
  }

  private func updateNowPlaying() {
    guard let core else { return }
    nowPlaying.update(
      MPVNowPlayingState(
        title: mediaTitle,
        duration: core.double("duration"),
        position: core.double("time-pos"),
        rate: facts.paused ? 0 : 1,
        isLive: !isSeekable))
  }

  private func emit(_ state: PlaybackState) {
    guard !isStopped, state != currentState else { return }
    currentState = state
    Logger.mpv.info("state -> \(String(describing: state), privacy: .public)")
    statesContinuation.yield(state)
    if state.isTerminal { statesContinuation.finish() }
  }

  // MARK: - System integration

  private func handle(_ interruption: MPVAudioInterruption) {
    switch interruption {
    case .began:
      wasPlayingBeforeInterruption = currentState == .playing
      pause()
    case .ended(let shouldResume):
      // Both conditions are needed. The system's opinion alone would resume a channel the viewer
      // had paused before Siri ever arrived; ours alone would talk over whatever the interruption
      // left playing.
      guard shouldResume, wasPlayingBeforeInterruption else { return }
      play()
    case .routeChanged:
      updateNowPlaying()
    }
  }

  private func handle(_ command: MPVTransportCommand) {
    switch command {
    case .play: play()
    case .pause: pause()
    case .togglePlayPause:
      if facts.paused { play() } else { pause() }
    case .seek(let seconds): seek(toSeconds: seconds)
    }
  }
}
