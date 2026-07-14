// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// The narrow core surface the **sources** slice needs (add / list / manage / import). Feature
/// code depends on this protocol, never the concrete `SpidolaCore`, so its view models are unit
/// tested against a fake (TECH_SPEC §10). `SpidolaCore` is the sole production conformer.
public protocol SourcesAccess: Sendable {
  func sources() async throws -> [Source]
  func addM3uUrl(name: String, url: String, userAgent: String?, acceptInvalidTls: Bool)
    async throws -> Source
  func addM3uFile(name: String) async throws -> Source
  func rename(id: Int64, name: String) async throws
  func setEnabled(id: Int64, enabled: Bool) async throws
  func setAutoRefresh(id: Int64, secs: UInt32?) async throws
  func deleteSource(id: Int64) async throws
  /// Fetches (over HTTP) and imports an M3U-by-URL source, streaming progress then one terminal
  /// event. Cancelling the consuming task cancels the core task at its next batch boundary.
  func importURL(id: Int64) -> AsyncStream<ImportEvent>
  /// Imports an M3U-from-file source from already-read `content` (picked file or pasted text),
  /// streaming progress then one terminal event.
  func importContent(id: Int64, content: String) -> AsyncStream<ImportEvent>
}

/// The narrow core surface the **browse** slice needs: the source → type → category → channel
/// drill-down (paged by contract), plus the per-channel context actions (favorite, hide) and the
/// play-time recents record.
public protocol BrowseAccess: Sendable {
  func sources() async throws -> [Source]
  func kinds(sourceId: Int64) async throws -> [MediaKind]
  func groups(sourceId: Int64, kind: MediaKind, offset: UInt32, limit: UInt32) async throws
    -> BrowseGroupPage
  func channelsInGroup(
    sourceId: Int64, kind: MediaKind, group: String?, offset: UInt32, limit: UInt32
  ) async throws -> ChannelPage
  func isFavorite(sourceId: Int64, identity: Int64) async throws -> Bool
  func setFavorite(sourceId: Int64, identity: Int64, favorite: Bool) async throws
  /// The stable identities of a source's favorites, so a channel list can mark them in one query
  /// rather than one `isFavorite` call per row.
  func favoriteIdentities(sourceId: Int64) async throws -> [Int64]
  func isHidden(sourceId: Int64, identity: Int64) async throws -> Bool
  func setHidden(sourceId: Int64, identity: Int64, hidden: Bool) async throws
  func recordRecent(_ channel: PlayableChannel) async throws
}

/// The narrow core surface the **search** slice needs: the ranked, paged query plus the source
/// list for the source filter.
public protocol SearchAccess: Sendable {
  func sources() async throws -> [Source]
  func search(query: String, sourceId: Int64?, kind: MediaKind?, offset: UInt32, limit: UInt32)
    async throws -> SearchPage
}

/// The narrow core surface the **home** screen needs: the favorites row, the recents row with its
/// off-switch, and the enabled source list to browse into.
public protocol HomeAccess: Sendable {
  func sources() async throws -> [Source]
  func favoriteChannels(offset: UInt32, limit: UInt32) async throws -> ChannelPage
  func recents(limit: UInt32) async throws -> [Recent]
  func recentsEnabled() async throws -> Bool
  func setRecentsEnabled(_ enabled: Bool) async throws
  func clearRecents() async throws
  func recordRecent(_ channel: PlayableChannel) async throws
}
