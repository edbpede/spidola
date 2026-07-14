// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import Foundation
import PlayerContract
import SwiftUI

/// The AVPlayer engine (TECH_SPEC §8): tvOS's alternate, for streams AVFoundation's native HLS
/// stack handles well.
///
/// The alternate, not the default — MPVKit is the platform default because its codec breadth is
/// wider. That ordering is what makes this engine's one architectural compromise acceptable; see
/// `assetOptions(for:)`.
@MainActor
public final class AVPlayerEngine: PlaybackEngine {
  public nonisolated let id = EngineID.avPlayer
  public nonisolated let states: AsyncStream<PlaybackState>

  public private(set) var tracks = TrackSelection()
  public private(set) var isSeekable = false

  private let player = AVPlayer()
  private let surface: VideoSurface
  private let continuation: AsyncStream<PlaybackState>.Continuation

  private var item: AVPlayerItem?
  private var catalog = AVTrackCatalog.empty
  private var observers: [Task<Void, Never>] = []
  private var current: PlaybackState = .idle
  private var wantsPlayback = true
  private var isStopped = false
  private var hasTerminated = false

  /// Whether the item's seekable window is worth a scrubber.
  ///
  /// The question is not "is this live". A live HLS stream with a DVR window is genuinely
  /// seekable inside it, and a VOD asset reports its whole duration in the same property, so one
  /// threshold answers both. It exists because a live stream *without* DVR still reports a
  /// seekable range — its three-segment sliding window, twenty-odd seconds — and a scrubber over
  /// twenty seconds of a live channel is a control that does nothing for the viewer. A minute is
  /// the judgement call: past any bare sliding window, under any real DVR window.
  private static let seekableWindowFloor: TimeInterval = 60

  /// How long `load` may sit in `.loading` before the engine calls it a `.timeout`.
  ///
  /// The engine has to own this, because AVPlayer does not. Given a URL that answers but never
  /// resolves into playable media, `AVPlayerItem.status` stays `.unknown` indefinitely: no status
  /// change, no error, no notification — nothing to observe, forever. Two cases were measured
  /// doing exactly that, and both are ordinary ways an IPTV source fails: a portal serving an
  /// HTML "subscription expired" page from the stream endpoint, and a portal serving real media
  /// from a server that ignores `Range` requests. Note that neither is an unsupported *format* —
  /// a container AVFoundation can recognize and reject (Matroska, say) fails properly through
  /// `status == .failed` in milliseconds. This deadline is for the streams that say nothing at
  /// all.
  ///
  /// Without it the shell sits on a spinner until the viewer gives up, which is precisely the
  /// "error with no action" TECH_SPEC §4.7 calls a design bug. `EngineError.timeout` names this
  /// case, and this is the only thing that raises it.
  ///
  /// Twenty seconds is well past PRD §9's two-second click-to-first-frame bar — it is not that
  /// budget's enforcement, which is an acceptance test's job. It is the point past which a stream
  /// is not slow but stuck, chosen so a genuinely sluggish portal on a bad night still gets to
  /// open rather than being cut off in front of a viewer who would have waited.
  ///
  /// Only `.loading` is guarded. A stream that opened and then starved is `.buffering` — a state
  /// the viewer can see and act on — so it is not this deadline's business.
  private static let loadDeadline: Duration = .seconds(20)

  /// Construction is free of I/O by contract: the zap path builds one of these per channel flip,
  /// so everything that costs anything waits for `load`.
  public init() {
    let (stream, continuation) = AsyncStream<PlaybackState>.makeStream(
      bufferingPolicy: .unbounded)
    self.states = stream
    self.continuation = continuation
    self.surface = VideoSurface(player: player)
    continuation.yield(.idle)
  }

  public func makeSurface() -> AnyView {
    AnyView(VideoSurfaceView(surface: surface))
  }

  public func load(_ request: StreamRequest) {
    guard !isStopped else { return }

    guard let url = URL(string: request.locator) else {
      // The core validates locators before they reach an engine, so this is a wiring bug rather
      // than a stream problem — but it is still one honest failure on the stream instead of a
      // trap, because a crash on the playback path is the worse of the two.
      AVEngineLog.logger.error("engine=avplayer load rejected reason=unparseable-locator")
      emit(.failed(.unknown(detail: "locator is not a URL")))
      return
    }

    AVEngineLog.logger.info(
      """
      engine=avplayer event=load buffering=\(request.buffering.rawValue, privacy: .public) \
      headers=\(request.loggableHeaderNames, privacy: .public) \
      userAgentOverride=\(request.hasUserAgentOverride, privacy: .public)
      """)

    emit(.loading)

    let tuning = AVBufferingTuning(request.buffering)
    let asset = AVURLAsset(url: url, options: assetOptions(for: request))
    let item = AVPlayerItem(asset: asset)
    item.preferredForwardBufferDuration = tuning.forwardBufferDuration
    player.automaticallyWaitsToMinimizeStalling = tuning.waitsToMinimizeStalling

    self.item = item
    player.replaceCurrentItem(with: item)
    observe(item)
    startLoadDeadline()
    loadTracks(from: asset, into: item)
    player.play()
  }

  /// Fails the stream if it never leaves `.loading`. See `loadDeadline`.
  private func startLoadDeadline() {
    observers.append(
      Task { [weak self] in
        try? await Task.sleep(for: Self.loadDeadline)
        guard let self, !Task.isCancelled, self.current == .loading else { return }
        AVEngineLog.logger.error("engine=avplayer event=load-deadline-expired")
        self.emit(.failed(.timeout))
      })
  }

  public func play() {
    guard !isStopped, let item else { return }
    wantsPlayback = true
    player.play()
    refresh(item)
  }

  public func pause() {
    guard !isStopped, let item else { return }
    wantsPlayback = false
    player.pause()
    refresh(item)
  }

  public func seek(toSeconds seconds: Double) {
    // `isSeekable` is the contract's promise that this is a no-op on a live stream, so the caller
    // does not guard. The finiteness check is separate: `CMTime(seconds:)` on a NaN traps.
    guard !isStopped, isSeekable, seconds.isFinite, seconds >= 0 else { return }
    player.seek(to: CMTime(seconds: seconds, preferredTimescale: 600))
  }

  public func select(track: TrackID) {
    guard !isStopped, let item,
      let handle = AVTrackHandle(trackID: track),
      let group = catalog.group(handle.group),
      let option = catalog.option(for: handle)
    else { return }
    item.select(option, in: group)
    tracks = catalog.selection(in: item)
  }

  public func clearSubtitle() {
    guard !isStopped, let item, let group = catalog.group(.legible) else { return }
    item.select(nil, in: group)
    tracks = catalog.selection(in: item)
  }

  public func setAspect(_ mode: AspectMode) {
    surface.gravity = mode.videoGravity
  }

  public func stop() {
    guard !isStopped else { return }
    isStopped = true
    for observer in observers { observer.cancel() }
    observers.removeAll()
    player.pause()
    player.replaceCurrentItem(with: nil)
    item = nil
    continuation.finish()
    AVEngineLog.logger.info("engine=avplayer event=stop")
  }

  /// The contract's backstop for a dropped reference. Without it the observer tasks — which hold
  /// the KVO streams, which retain the player — outlive the engine, and playback continues with no
  /// owner left to stop it. `isolated deinit` runs on the main actor, so this is the same
  /// idempotent `stop()` the shell calls, not a second teardown path.
  isolated deinit {
    stop()
  }

  // MARK: - Asset options

  /// AVPlayer's asset options for `request`.
  ///
  /// The user-agent goes through `AVURLAssetHTTPUserAgentKey`, which is public API (tvOS 16+).
  ///
  /// The header map does not, and this is the engine's one deliberate compromise:
  /// `AVURLAssetHTTPHeaderFieldsKey` is declared in no AVFoundation header, and Apple documents
  /// no supported way to add request headers to an `AVURLAsset`. It is used here anyway, with the
  /// key written as a string literal because it cannot be imported, for two reasons. Spidola's
  /// sources need it — `Referer` and friends are how a large share of IPTV portals gate their
  /// streams, so an engine that cannot send headers cannot play the catalogue. And the only
  /// supported alternative, an `AVAssetResourceLoader` delegate proxying every segment request,
  /// would mean re-implementing HLS fetching in the shell and would forfeit precisely the
  /// native-HLS behaviour this engine exists to provide.
  ///
  /// The risk is real, and worth stating plainly rather than burying: a future tvOS release can
  /// stop honouring this key with no deprecation and no compile error. When that happens,
  /// header-gated streams start failing with 401/403, which this engine maps to `.unauthorized`.
  /// That failure is at least honest — `.unauthorized` does not offer "Try other player"
  /// (`EngineError.offersOtherPlayer`), so the viewer is not sent in a circle, and MPVKit, the
  /// platform default, sends the headers correctly and plays the channel. A silent breakage in
  /// the *alternate* engine costs a viewer who overrode the default; the same breakage in the
  /// default would cost everyone. That asymmetry is what makes this acceptable here and would not
  /// make it acceptable there.
  private func assetOptions(for request: StreamRequest) -> [String: any Sendable] {
    var options: [String: any Sendable] = [:]
    if let userAgent = request.userAgent {
      options[AVURLAssetHTTPUserAgentKey] = userAgent
    }
    if !request.headers.isEmpty {
      let fields = Dictionary(
        request.headers.map { ($0.name, $0.value) },
        uniquingKeysWith: { _, last in last })
      options["AVURLAssetHTTPHeaderFieldsKey"] = fields
    }
    return options
  }

  // MARK: - Observation

  /// Wires the item's KVO signals into structured tasks.
  ///
  /// Each observer re-derives the whole state rather than emitting its own verdict — see
  /// `derivedState(for:)`. The tasks are stored so `stop()` can cancel them, which terminates
  /// each stream, which invalidates its observation.
  private func observe(_ item: AVPlayerItem) {
    watch(keyValueStream(item, \.status), of: item)
    watch(keyValueStream(item, \.isPlaybackLikelyToKeepUp), of: item)
    watch(keyValueStream(item, \.isPlaybackBufferEmpty), of: item)
    watch(keyValueStream(player, \.timeControlStatus), of: item)

    observers.append(
      Task { [weak self] in
        let ended = NotificationCenter.default
          .notifications(named: AVPlayerItem.didPlayToEndTimeNotification, object: item)
          .map { _ in () }
        for await _ in ended {
          guard let self else { return }
          self.emit(.ended)
        }
      })
  }

  private func watch<Signal: Sendable>(_ stream: AsyncStream<Signal>, of item: AVPlayerItem) {
    observers.append(
      Task { [weak self] in
        for await _ in stream {
          guard let self else { return }
          self.refresh(item)
        }
      })
  }

  private func loadTracks(from asset: AVURLAsset, into item: AVPlayerItem) {
    observers.append(
      Task { [weak self] in
        let catalog = await AVTrackCatalog.load(from: asset)
        guard let self, !self.isStopped else { return }
        self.catalog = catalog
        self.tracks = catalog.selection(in: item)
      })
  }

  // MARK: - State machine

  private func refresh(_ item: AVPlayerItem) {
    guard !isStopped, item === self.item else { return }
    updateSeekability(of: item)
    emit(derivedState(for: item))
  }

  /// The single place playback state is decided.
  ///
  /// Every observer re-derives from a snapshot rather than emitting its own verdict, because the
  /// signals AVPlayer publishes contradict each other constantly during start-up:
  /// `isPlaybackLikelyToKeepUp` goes true a beat before `timeControlStatus` leaves
  /// `.waitingToPlayAtSpecifiedRate`, and `isPlaybackBufferEmpty` flaps either side of both. A
  /// per-observer emit would race those onto the stream in whatever order KVO happened to fire,
  /// and the shell would show a spinner over playing video. Deriving from a snapshot means the
  /// firing order cannot matter.
  private func derivedState(for item: AVPlayerItem) -> PlaybackState {
    switch item.status {
    case .readyToPlay: break
    case .failed: return .failed(failure(of: item))
    case .unknown: return .loading
    @unknown default: return .loading
    }
    guard wantsPlayback else { return .paused }
    if item.isPlaybackBufferEmpty || !item.isPlaybackLikelyToKeepUp { return .buffering }
    return player.timeControlStatus == .playing ? .playing : .buffering
  }

  /// The engine error for a failed item: the error log's HTTP status first, then the error chain.
  ///
  /// Status first because it is the only place an HTTP 401/403 is legible. The `NSError` for an
  /// HLS variant the origin refused carries an undocumented `CoreMediaErrorDomain` code, and
  /// Apple publishes no mapping from those codes to statuses — but the log event carries the
  /// status itself, so an auth failure gets named instead of collapsing into `.unknown` and
  /// showing the viewer "something went wrong" for a password they could fix.
  ///
  /// The log is scanned newest-first: a live stream accumulates events for variants it recovered
  /// from, and the fatal one is the last.
  private func failure(of item: AVPlayerItem) -> EngineError {
    let statuses = (item.errorLog()?.events ?? []).map(\.errorStatusCode)
    for status in statuses.reversed() {
      if let verdict = AVErrorMapping.engineError(httpStatusCode: status) { return verdict }
    }
    if let error = item.error {
      return AVErrorMapping.engineError(from: error as NSError)
    }
    return .unknown(detail: "AVPlayerItem reported .failed with no error")
  }

  private func updateSeekability(of item: AVPlayerItem) {
    isSeekable = item.seekableTimeRanges
      .map(\.timeRangeValue.duration.seconds)
      .contains { $0.isFinite && $0 >= Self.seekableWindowFloor }
  }

  private func emit(_ state: PlaybackState) {
    guard !isStopped, !hasTerminated else { return }
    // KVO re-fires the same value freely, and four observers re-derive from one change; without
    // this the shell would see a dozen identical `.buffering` transitions per second.
    guard state != current else { return }
    current = state
    log(state)
    continuation.yield(state)
    if state.isTerminal {
      hasTerminated = true
      continuation.finish()
    }
  }

  private func log(_ state: PlaybackState) {
    guard let error = state.failure else {
      AVEngineLog.logger.info("engine=avplayer state=\(state.logLabel, privacy: .public)")
      return
    }
    // The class is a fixed token and safe to read in the clear; the detail is free-form framework
    // text, which §4.8 defaults to private.
    AVEngineLog.logger.error(
      """
      engine=avplayer state=failed class=\(error.logLabel, privacy: .public) \
      detail=\(error.diagnosticDetail ?? "-", privacy: .private)
      """)
  }
}
