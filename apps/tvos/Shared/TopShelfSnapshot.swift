// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation

/// The small, versioned projection shared with the Top Shelf extension.
///
/// This is display metadata only. Stream locators, request headers, credentials, and guide state
/// remain in the core-owned database and never enter the app-group container.
struct TopShelfSnapshot: Codable, Equatable, Sendable {
  struct Item: Codable, Equatable, Sendable {
    let sourceId: Int64
    let identity: Int64
    let title: String
    let group: String?
    let imageURL: URL?

    var identifier: String { "\(sourceId)-\(identity)" }
    var deepLink: URL? { URL(string: "spidola://channel/\(sourceId)/\(identity)") }
  }

  let version: UInt8
  let generatedAt: Date
  let favorites: [Item]

  init(generatedAt: Date = Date(), favorites: [Item]) {
    self.version = Self.currentVersion
    self.generatedAt = generatedAt
    self.favorites = Array(favorites.prefix(Self.maximumItemCount))
  }

  static let currentVersion: UInt8 = 1
  var isSupported: Bool { version == Self.currentVersion }
  static let maximumItemCount = 12
}

enum TopShelfSnapshotStore {
  static let appGroupIdentifier = "group.dev.spidola.tv"

  static func read(fileManager: FileManager = .default) throws -> TopShelfSnapshot? {
    guard let fileURL = fileURL(fileManager: fileManager) else { return nil }
    guard fileManager.fileExists(atPath: fileURL.path) else { return nil }
    let snapshot = try decoder.decode(TopShelfSnapshot.self, from: Data(contentsOf: fileURL))
    return snapshot.isSupported ? snapshot : nil
  }

  static func write(_ snapshot: TopShelfSnapshot, fileManager: FileManager = .default) throws {
    guard let fileURL = fileURL(fileManager: fileManager) else {
      throw StoreError.appGroupUnavailable
    }
    try encoder.encode(snapshot).write(to: fileURL, options: .atomic)
  }

  private static func fileURL(fileManager: FileManager) -> URL? {
    fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier)?
      .appending(path: "top-shelf-snapshot.json", directoryHint: .notDirectory)
  }

  private static let encoder: JSONEncoder = {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.sortedKeys]
    return encoder
  }()

  private static let decoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return decoder
  }()

  private enum StoreError: Error {
    case appGroupUnavailable
  }
}
