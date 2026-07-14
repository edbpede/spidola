// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// The flat, `Hashable` channel currency the shell navigates and plays with. A browse `Channel`, a
/// `Favorite`-resolved channel, and a `Recent` all map to it, so one detail/play path serves every
/// entry point (home rails, drill-down, search). It carries exactly what the shell needs to
/// present, favorite/hide, and (Phase 5) play a channel — never business state, which stays in the
/// core.
public struct PlayableChannel: Hashable, Sendable, Identifiable {
  public let sourceId: Int64
  /// Stable per-source identity (favorites/hidden/recents key on this), not the churny rowid.
  public let identity: Int64
  public let name: String
  public let group: String?
  public let logo: String?
  public let locator: String

  /// Stable across a refresh, so it doubles as the SwiftUI list/focus identity.
  public var id: String { "\(sourceId)-\(identity)" }

  public init(
    sourceId: Int64, identity: Int64, name: String, group: String?, logo: String?, locator: String
  ) {
    self.sourceId = sourceId
    self.identity = identity
    self.name = name
    self.group = group
    self.logo = logo
    self.locator = locator
  }

  public init(_ channel: Channel) {
    self.init(
      sourceId: channel.sourceId,
      identity: channel.identity,
      name: channel.name,
      group: channel.groupTitle,
      logo: channel.logo,
      locator: channel.locator)
  }

  /// A recently-watched entry snapshots name + locator at play time, so it stays replayable even
  /// if the channel later left the catalog; it carries no group or logo.
  public init(_ recent: Recent) {
    self.init(
      sourceId: recent.sourceId,
      identity: recent.identity,
      name: recent.name,
      group: nil,
      logo: nil,
      locator: recent.locator)
  }
}
