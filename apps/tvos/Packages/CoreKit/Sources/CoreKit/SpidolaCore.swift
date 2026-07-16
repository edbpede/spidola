// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import core_api

/// Reads the source list and a source's channel catalog one page at a time (paged by contract,
/// TECH_SPEC §5). A narrow surface so a view model can be tested against a fake instead of the
/// real core.
public protocol CatalogAccess: Sendable {
  func sources() async throws -> [Source]
  func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage
}

/// One event from a running import; the stream terminates on `.complete` or `.failed`.
public enum ImportEvent: Sendable {
  case progress(ImportProgress)
  case complete(ImportOutcome)
  case failed(ApiError)
}

/// The single Swift-side handle on the Rust core (TECH_SPEC §5, §6). It wraps the generated
/// `Core`, conforms to the narrow per-feature access protocols the vertical slices depend on, and
/// bridges the import callback interface into an `AsyncStream` whose termination cancels the
/// core's task handle. UniFFI async methods arrive back on the caller's continuation; callback
/// events are trampolined to the caller's isolation by the stream.
public final class SpidolaCore: CatalogAccess, SourcesAccess, BrowseAccess, SearchAccess,
  HomeAccess, PlaybackAccess, SettingsAccess, PairingAccess, EpgAccess, CustomChannelsAccess,
  FavoriteOrderingAccess
{
  private let core: Core

  public init(
    dbPath: String, logDirectives: String, secrets: SecretStore, logSink: LogSink
  ) throws {
    core = try Core(
      config: CoreConfig(dbPath: dbPath, logDirectives: logDirectives),
      secrets: secrets,
      logSink: logSink
    )
  }

  /// The startup handshake (core / schema / boundary versions), checked before first use.
  public func handshake() -> Handshake { core.handshake() }

  // MARK: - Sources

  public func sources() async throws -> [Source] { try await core.sources().list() }

  public func addM3uUrl(
    name: String, url: String, userAgent: String?, acceptInvalidTls: Bool
  ) async throws -> Source {
    try await core.sources().addM3uUrl(
      name: name, url: url, userAgent: userAgent, acceptInvalidTls: acceptInvalidTls)
  }

  /// Convenience for the fixture seeder and simple add flows (no user-agent, platform TLS).
  public func addM3uUrl(name: String, url: String) async throws -> Source {
    try await addM3uUrl(name: name, url: url, userAgent: nil, acceptInvalidTls: false)
  }

  public func addM3uFile(name: String) async throws -> Source {
    try await core.sources().addM3uFile(name: name)
  }

  public func addXtream(
    name: String, server: String, username: String, password: String
  ) async throws -> Source {
    try await core.sources().addXtream(
      name: name, server: server, username: username, password: password)
  }

  public func rename(id: Int64, name: String) async throws {
    try await core.sources().rename(id: id, name: name)
  }

  public func setEnabled(id: Int64, enabled: Bool) async throws {
    try await core.sources().setEnabled(id: id, enabled: enabled)
  }

  public func setAutoRefresh(id: Int64, secs: UInt32?) async throws {
    try await core.sources().setAutoRefresh(id: id, secs: secs)
  }

  public func deleteSource(id: Int64) async throws {
    try await core.sources().delete(id: id)
  }

  public func importURL(id: Int64) -> AsyncStream<ImportEvent> {
    importStream { [core] listener in core.sources().refresh(id: id, listener: listener) }
  }

  public func importContent(id: Int64, content: String) -> AsyncStream<ImportEvent> {
    importStream { [core] listener in
      core.sources().importM3uContent(id: id, content: content, listener: listener)
    }
  }

  /// Kept for the M0 fixture seeder, which imports an M3U-by-URL source. Same as `importURL`.
  public func importSource(id: Int64) -> AsyncStream<ImportEvent> { importURL(id: id) }

  /// Builds the import `AsyncStream` shared by URL and content imports: it registers a listener,
  /// starts the core task, and cancels that task if the consuming task terminates early.
  private func importStream(
    _ start: @escaping @Sendable (ImportListener) -> TaskHandle
  ) -> AsyncStream<ImportEvent> {
    AsyncStream { continuation in
      let listener = ImportListenerAdapter(continuation: continuation)
      let handle = start(listener)
      continuation.onTermination = { _ in handle.cancel() }
    }
  }

  // MARK: - Pairing

  /// Brings the LAN pairing server up and streams its session and submissions.
  ///
  /// Termination is the point: when the consuming task ends — the pairing screen closing — the
  /// stream's `onTermination` stops the server. "Alive only while its screen is visible"
  /// (TECH_SPEC §12) therefore holds by construction rather than by a screen remembering to say
  /// so. The `start` runs in its own task because it must not block the caller before the first
  /// event, and its failure is delivered as `.failed` rather than thrown, so one arm handles a
  /// server that never came up and one that came up and later fell over.
  public func pairing(host: String?) -> AsyncStream<PairingEvent> {
    AsyncStream { continuation in
      let listener = PairingListenerAdapter(continuation: continuation)
      let task = Task { [core] in
        do {
          let session = try await core.pairing().start(host: host, listener: listener)
          continuation.yield(.started(session))
        } catch let error as ApiError {
          continuation.yield(.failed(error))
          continuation.finish()
        } catch {
          continuation.yield(.failed(.Internal))
          continuation.finish()
        }
      }
      continuation.onTermination = { [core] _ in
        task.cancel()
        // Detached because termination is synchronous and the core's stop is not. Dropping the
        // server would close the socket anyway; awaiting it here is what makes the stop prompt.
        Task { await core.pairing().stop() }
      }
    }
  }

  public func stopPairing() async {
    await core.pairing().stop()
  }

  // MARK: - Catalog / browse

  public func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try await core.catalog().channels(sourceId: sourceId, offset: offset, limit: limit)
  }

  public func kinds(sourceId: Int64) async throws -> [MediaKind] {
    try await core.catalog().kinds(sourceId: sourceId)
  }

  public func groups(
    sourceId: Int64, kind: MediaKind, offset: UInt32, limit: UInt32
  ) async throws -> BrowseGroupPage {
    try await core.catalog().groups(sourceId: sourceId, kind: kind, offset: offset, limit: limit)
  }

  public func channelsInGroup(
    sourceId: Int64, kind: MediaKind, group: String?, offset: UInt32, limit: UInt32
  ) async throws -> ChannelPage {
    try await core.catalog().channelsInGroup(
      sourceId: sourceId, kind: kind, group: group, offset: offset, limit: limit)
  }

  public func isHidden(sourceId: Int64, identity: Int64) async throws -> Bool {
    try await core.catalog().isHidden(sourceId: sourceId, identity: identity)
  }

  public func setHidden(sourceId: Int64, identity: Int64, hidden: Bool) async throws {
    try await core.catalog().setHidden(sourceId: sourceId, identity: identity, hidden: hidden)
  }

  // MARK: - Favorites

  public func isFavorite(sourceId: Int64, identity: Int64) async throws -> Bool {
    try await core.favorites().isFavorite(sourceId: sourceId, identity: identity)
  }

  public func setFavorite(sourceId: Int64, identity: Int64, favorite: Bool) async throws {
    if favorite {
      try await core.favorites().add(sourceId: sourceId, identity: identity)
    } else {
      try await core.favorites().remove(sourceId: sourceId, identity: identity)
    }
  }

  public func favoriteIdentities(sourceId: Int64) async throws -> [Int64] {
    try await core.favorites().list(sourceId: sourceId).map(\.identity)
  }

  public func favoriteChannels(offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try await core.favorites().favoriteChannels(offset: offset, limit: limit)
  }

  public func favoriteLineup(offset: UInt32, limit: UInt32) async throws -> [PlayableChannel] {
    try await favoriteChannels(offset: offset, limit: limit).channels.map(PlayableChannel.init)
  }

  public func moveFavoriteBefore(_ channel: PlayableChannel, anchor: PlayableChannel) async throws {
    try await core.favorites().moveBefore(
      sourceId: channel.sourceId, identity: channel.identity, anchorSourceId: anchor.sourceId,
      anchorIdentity: anchor.identity)
  }

  public func moveFavoriteAfter(_ channel: PlayableChannel, anchor: PlayableChannel) async throws {
    try await core.favorites().moveAfter(
      sourceId: channel.sourceId, identity: channel.identity, anchorSourceId: anchor.sourceId,
      anchorIdentity: anchor.identity)
  }

  // MARK: - Programme guide

  public func nowNext(sourceId: Int64, channelIdentity: Int64, now: Date) async throws -> NowNext {
    try await core.epg().nowNext(
      sourceId: sourceId, channelIdentity: channelIdentity,
      nowUnix: Int64(now.timeIntervalSince1970))
  }

  public func nowNextBatch(
    sourceId: Int64, channelIdentities: [Int64], now: Date
  ) async throws -> [ChannelNowNext] {
    try await core.epg().nowNextBatch(
      sourceId: sourceId, channelIdentities: channelIdentities,
      nowUnix: Int64(now.timeIntervalSince1970))
  }

  public func epgWindow(_ query: EpgWindowQuery) async throws -> EpgPage {
    try await core.epg().window(
      sourceId: query.sourceId, channelIdentity: query.channelIdentity,
      earliestUnix: Int64(query.earliest.timeIntervalSince1970),
      latestUnix: Int64(query.latest.timeIntervalSince1970), offset: query.offset,
      limit: query.limit)
  }

  public func hasEpgFeed(sourceId: Int64) async throws -> Bool {
    try await core.epg().hasFeed(sourceId: sourceId)
  }

  public func setXmltvFeed(sourceId: Int64, url: String) async throws {
    try await core.epg().setXmltvFeed(sourceId: sourceId, url: url)
  }

  public func clearXmltvFeed(sourceId: Int64) async throws {
    try await core.epg().clearXmltvFeed(sourceId: sourceId)
  }

  public func refreshEpg(sourceId: Int64, now: Date) -> AsyncStream<EpgRefreshEvent> {
    AsyncStream { continuation in
      let listener = EpgRefreshListenerAdapter(continuation: continuation)
      let handle = core.epg().refresh(
        sourceId: sourceId, nowUnix: Int64(now.timeIntervalSince1970), listener: listener)
      continuation.onTermination = { _ in handle.cancel() }
    }
  }

  // MARK: - Custom channels

  public func customGroups() async throws -> [CustomGroup] {
    var groups: [CustomGroup] = []
    var page: [CustomGroup]
    repeat {
      page = try await core.customChannels().groups(offset: UInt32(groups.count), limit: 200)
      groups.append(contentsOf: page)
    } while page.count == 200
    return groups
  }

  public func customChannels(groupId: Int64?) async throws -> [CustomChannelSummary] {
    var channels: [CustomChannelSummary] = []
    var page: [CustomChannelSummary]
    repeat {
      page = try await core.customChannels().list(
        groupId: groupId, offset: UInt32(channels.count), limit: 200)
      channels.append(contentsOf: page)
    } while page.count == 200
    return channels
  }

  public func createCustomChannel(_ input: CustomChannelInput) async throws -> Int64 {
    try await core.customChannels().create(draft: customDraft(input))
  }

  public func updateCustomChannel(id: Int64, input: CustomChannelInput) async throws {
    try await core.customChannels().update(id: id, draft: customDraft(input))
  }

  public func deleteCustomChannel(id: Int64) async throws {
    try await core.customChannels().delete(id: id)
  }

  public func moveCustomChannelBefore(id: Int64, anchorId: Int64) async throws {
    try await core.customChannels().moveBefore(id: id, anchorId: anchorId)
  }

  public func moveCustomChannelAfter(id: Int64, anchorId: Int64) async throws {
    try await core.customChannels().moveAfter(id: id, anchorId: anchorId)
  }

  public func createCustomGroup(name: String) async throws -> Int64 {
    try await core.customChannels().createGroup(name: name)
  }

  public func renameCustomGroup(id: Int64, name: String) async throws {
    try await core.customChannels().renameGroup(id: id, name: name)
  }

  public func deleteCustomGroup(id: Int64) async throws {
    try await core.customChannels().deleteGroup(id: id)
  }

  public func moveCustomGroupBefore(id: Int64, anchorId: Int64) async throws {
    try await core.customChannels().moveGroupBefore(id: id, anchorId: anchorId)
  }

  public func moveCustomGroupAfter(id: Int64, anchorId: Int64) async throws {
    try await core.customChannels().moveGroupAfter(id: id, anchorId: anchorId)
  }

  public func exportCustomChannels() async throws -> String {
    try await core.customChannels().exportPortable().contents()
  }

  public func importCustomChannels(_ contents: String, mode: CustomImportMode) async throws
    -> UInt64
  {
    try await core.customChannels().importPortable(contents: contents, mode: mode)
  }

  private func customDraft(_ input: CustomChannelInput) -> CustomChannelDraft {
    CustomChannelDraft(
      groupId: input.groupId, name: input.name, logo: input.logo.nilIfBlank,
      locator: input.streamAddress, userAgent: input.userAgent.nilIfBlank,
      headers: input.headers.filter { !$0.name.isEmpty || !$0.value.isEmpty }.map {
        ResolvedHeader.fromParts(name: $0.name, value: $0.value)
      })
  }

  // MARK: - Recents

  public func recents(limit: UInt32) async throws -> [Recent] {
    try await core.recents().list(limit: limit)
  }

  public func recentsEnabled() async throws -> Bool {
    try await core.recents().isEnabled()
  }

  public func setRecentsEnabled(_ enabled: Bool) async throws {
    try await core.recents().setEnabled(enabled: enabled)
  }

  public func clearRecents() async throws {
    try await core.recents().clear()
  }

  public func recordRecent(_ channel: PlayableChannel) async throws {
    try await core.recents().record(
      sourceId: channel.sourceId,
      identity: channel.identity,
      name: channel.name,
      locator: channel.locator,
      positionSecs: nil)
  }

  // MARK: - Settings

  public func settingsSnapshot() async throws -> AppSettings {
    try await core.settings().snapshot()
  }

  public func setDefaultEngine(_ engine: String?) async throws {
    try await core.settings().setDefaultEngine(engine: engine)
  }

  public func setBuffering(_ profile: BufferingProfile) async throws {
    try await core.settings().setBuffering(profile: profile)
  }

  public func setSubtitleSize(_ size: SubtitleSize) async throws {
    try await core.settings().setSubtitleSize(size: size)
  }

  public func setSubtitleBackground(_ background: SubtitleBackground) async throws {
    try await core.settings().setSubtitleBackground(background: background)
  }

  public func setLanguage(_ tag: String?) async throws {
    try await core.settings().setLanguage(tag: tag)
  }

  public func setDensity(_ density: InterfaceDensity) async throws {
    try await core.settings().setDensity(density: density)
  }

  public func setRecentsRetentionDays(_ days: UInt32) async throws {
    try await core.settings().setRecentsRetentionDays(days: days)
  }

  public func setImageCacheMaxMb(_ megabytes: UInt32) async throws {
    try await core.settings().setImageCacheMaxMb(megabytes: megabytes)
  }

  public func setLogLevel(_ level: LogLevel) async throws {
    try await core.settings().setLogLevel(level: level)
  }

  /// The core's in-memory log ring (TECH_SPEC §4.8), already redacted core-side.
  public func exportLogs() -> [String] { core.exportLogs() }

  // MARK: - Search

  public func search(
    query: String, sourceId: Int64?, kind: MediaKind?, offset: UInt32, limit: UInt32
  ) async throws -> SearchPage {
    try await core.search().search(
      query: query, sourceId: sourceId, kind: kind, offset: offset, limit: limit)
  }

  // MARK: - Playback

  /// Fetches the three-row window centred on `offset` from whichever paged query `context` names.
  /// One page, regardless of ring size — this is what keeps zap O(1) at 50k channels (PRD §9).
  public func zapWindow(context: ZapContext, offset: UInt32) async throws -> ZapWindow? {
    // A window at offset 0 starts at 0 and has no previous row; elsewhere it starts one back, so
    // `current` sits in the middle.
    let start = offset == 0 ? 0 : offset - 1
    let limit: UInt32 = offset == 0 ? 2 : 3
    let (channels, total) = try await ring(context, offset: start, limit: limit)

    // `current` is the first row when the window could not step back, the second otherwise.
    let currentIndex = Int(offset - start)
    guard currentIndex < channels.count else { return nil }
    return ZapWindow(
      previous: currentIndex > 0 ? channels[currentIndex - 1] : nil,
      current: channels[currentIndex],
      next: currentIndex + 1 < channels.count ? channels[currentIndex + 1] : nil,
      offset: offset,
      total: total)
  }

  private func ring(_ context: ZapContext, offset: UInt32, limit: UInt32) async throws -> (
    [PlayableChannel], UInt64?
  ) {
    switch context {
    case .single:
      return ([], nil)
    case .group(let sourceId, let kind, let group):
      let page = try await core.catalog().channelsInGroup(
        sourceId: sourceId, kind: kind, group: group, offset: offset, limit: limit)
      return (page.channels.map(PlayableChannel.init), page.total)
    case .favorites:
      let page = try await core.favorites().favoriteChannels(offset: offset, limit: limit)
      return (page.channels.map(PlayableChannel.init), page.total)
    case .search(let query, let sourceId, let kind):
      let page = try await core.search().search(
        query: query, sourceId: sourceId, kind: kind, offset: offset, limit: limit)
      return (page.channels.map(PlayableChannel.init), nil)
    }
  }

  public func channelEngine(sourceId: Int64, identity: Int64) async throws -> String? {
    try await core.settings().engineForChannel(sourceId: sourceId, identity: identity)
  }

  public func setChannelEngine(sourceId: Int64, identity: Int64, engine: String?) async throws {
    try await core.settings().setEngineForChannel(
      sourceId: sourceId, identity: identity, engine: engine)
  }

  public func sourceEngine(sourceId: Int64) async throws -> String? {
    try await core.settings().engineForSource(sourceId: sourceId)
  }

  // The playback slice speaks `PlayerContract.BufferingProfile` and the core speaks its own
  // mirror of it; the two carry identical cases and identical stored spellings, so this adapter
  // is the one seam that translates between them. `PlaybackAccess` deliberately keeps the raw
  // value rather than either enum — CoreKit does not depend on PlayerContract (the dependency
  // runs the other way), and pushing a core FFI type through would make the playback slice
  // depend on the boundary's shape.
  //
  // Never `nil` in practice: the core resolves the profile through its default, so the optional
  // exists only because `PlaybackAccess` predates the typed settings vocabulary.
  public func bufferingProfile() async throws -> String? {
    try await core.settings().snapshot().buffering.playbackKey
  }

  public func setBufferingProfile(_ profile: String) async throws {
    try await core.settings().setBuffering(profile: .init(playbackKey: profile))
  }

  public func resolvePlayback(_ channel: PlayableChannel) async throws -> ResolvedPlaybackStream {
    let resolved = try await core.sources().resolvePlayback(
      sourceId: channel.sourceId,
      identity: channel.identity,
      locator: channel.locator)
    return ResolvedPlaybackStream(
      locator: resolved.locator(),
      userAgent: resolved.userAgent(),
      headers: resolved.headers().map {
        ResolvedPlaybackHeader(name: $0.name(), value: $0.value())
      })
  }

  public func resolveCustomPlayback(_ channel: CustomPlayableChannel) async throws
    -> ResolvedPlaybackStream
  {
    let resolved = try await core.customChannels().resolve(id: channel.id)
    return ResolvedPlaybackStream(
      locator: resolved.locator(),
      userAgent: resolved.userAgent(),
      headers: resolved.headers().map {
        ResolvedPlaybackHeader(name: $0.name(), value: $0.value())
      })
  }
}

/// Bridges the UniFFI `ImportListener` callback (which may arrive on any core thread) onto the
/// import `AsyncStream`. The continuation is `Sendable`, so no lock is needed.
private final class ImportListenerAdapter: ImportListener {
  private let continuation: AsyncStream<ImportEvent>.Continuation

  init(continuation: AsyncStream<ImportEvent>.Continuation) {
    self.continuation = continuation
  }

  func onProgress(progress: ImportProgress) {
    continuation.yield(.progress(progress))
  }

  func onComplete(outcome: ImportOutcome) {
    continuation.yield(.complete(outcome))
    continuation.finish()
  }

  func onFailed(error: ApiError) {
    continuation.yield(.failed(error))
    continuation.finish()
  }
}

/// Bridges the UniFFI `PairingListener` callback (which arrives on the pairing server's connection
/// task — any core thread, TECH_SPEC §5) onto the pairing `AsyncStream`. The continuation is
/// `Sendable` and never blocks, which matters here: a phone is waiting on a response behind this
/// call.
private final class PairingListenerAdapter: PairingListener {
  private let continuation: AsyncStream<PairingEvent>.Continuation

  init(continuation: AsyncStream<PairingEvent>.Continuation) {
    self.continuation = continuation
  }

  func onSubmission(submission: PairingSubmission) {
    continuation.yield(.submission(submission))
  }
}

private final class EpgRefreshListenerAdapter: EpgRefreshListener {
  private let continuation: AsyncStream<EpgRefreshEvent>.Continuation

  init(continuation: AsyncStream<EpgRefreshEvent>.Continuation) {
    self.continuation = continuation
  }

  func onProgress(progress: EpgRefreshProgress) {
    continuation.yield(.progress(progress))
  }

  func onComplete(outcome: EpgRefreshOutcome) {
    continuation.yield(.complete(outcome))
    continuation.finish()
  }

  func onFailed(error: ApiError) {
    continuation.yield(.failed(error))
    continuation.finish()
  }
}

extension String {
  fileprivate var nilIfBlank: String? {
    let value = trimmingCharacters(in: .whitespacesAndNewlines)
    return value.isEmpty ? nil : value
  }
}

extension Source {
  /// The stable rowid of a source, regardless of its kind. The `@unknown default` reserves the
  /// "unknown future variant" arm the FFI boundary rules require (TECH_SPEC §5).
  public var id: Int64 {
    switch self {
    case .m3uUrl(let id, _, _, _): id
    case .m3uFile(let id, _): id
    case .xtream(let id, _, _, _, _): id
    @unknown default: -1
    }
  }

  /// The user-facing source name shared by every source kind.
  public var name: String {
    switch self {
    case .m3uUrl(_, let common, _, _): common.name
    case .m3uFile(_, let common): common.name
    case .xtream(_, let common, _, _, _): common.name
    @unknown default: ""
    }
  }

  /// The common per-source settings (enabled, auto-refresh) shared by every kind.
  public var common: SourceCommon {
    switch self {
    case .m3uUrl(_, let common, _, _): common
    case .m3uFile(_, let common): common
    case .xtream(_, let common, _, _, _): common
    @unknown default: SourceCommon(name: "", enabled: true, autoRefreshSecs: nil)
    }
  }

  /// Whether this source can be refreshed from a URL (M3U-by-URL and Xtream). File sources are
  /// import-once and re-import from a freshly picked/pasted file instead.
  public var isRefreshable: Bool {
    switch self {
    case .m3uUrl: true
    case .xtream: true
    case .m3uFile: false
    @unknown default: false
    }
  }

  /// A couch-legible one-word description of the source kind, for the sources list.
  public var kindLabel: String {
    switch self {
    case .m3uUrl: "Playlist URL"
    case .m3uFile: "Playlist file"
    case .xtream: "Xtream account"
    @unknown default: "Source"
    }
  }
}
