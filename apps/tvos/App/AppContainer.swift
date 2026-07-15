// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Darwin
import Foundation
import OSLog
import PlayerAV
import PlayerContract
import PlayerMPV

/// The composition root's single wiring point for the core (TECH_SPEC §3.1: composition happens
/// only at the app shell). Manual constructor wiring: the core is the one durable source of truth,
/// created once with the Keychain secrets store and OSLog sink installed here and nowhere else.
/// For the M0 walking skeleton it also seeds a fixture catalog through the core so browse has
/// content — mirroring the real add-source flow, which replaces it in Phase 4.
@MainActor
final class AppContainer {
  let core: SpidolaCore
  let registry: EngineRegistry

  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::boot")

  /// The engines this build can construct (TECH_SPEC §8): MPVKit the default for its codec breadth,
  /// AVPlayer the alternate for HLS-native content. Engines are peers wired here, never children of
  /// the playback slice (doctrine §3.1) — which is what keeps `FeaturePlayback` free of any decoder
  /// dependency. Each factory builds a fresh engine, because zapping disposes and rebuilds one per
  /// channel flip.
  private static func makeRegistry() -> EngineRegistry {
    EngineRegistry(
      platformDefault: .mpv,
      factories: [
        .mpv: { MPVEngine() },
        .avPlayer: { AVPlayerEngine() },
      ])
  }

  init() {
    self.registry = Self.makeRegistry()
    let dbPath = URL.documentsDirectory.appending(path: "spidola.sqlite").path()
    do {
      let core = try SpidolaCore(
        dbPath: dbPath,
        logDirectives: "info,spidola=debug",
        secrets: KeychainSecretStore(),
        logSink: OSLogSink()
      )
      let handshake = core.handshake()
      let coreVersion = handshake.coreVersion
      let schemaVersion = handshake.schemaVersion
      let boundaryVersion = handshake.boundaryVersion
      guard
        coreVersion.isEmpty == false,
        schemaVersion == Self.supportedSchemaVersion,
        boundaryVersion == Self.supportedBoundaryVersion
      else {
        fatalError(
          "Incompatible core: \(coreVersion), schema \(schemaVersion), boundary \(boundaryVersion)"
        )
      }
      logger.info(
        "core \(coreVersion, privacy: .public), schema \(schemaVersion), boundary \(boundaryVersion)"
      )
      self.core = core
    } catch {
      // A failed boundary handshake is unrecoverable (TECH_SPEC §5): fail fast and legibly.
      fatalError("Spidola core failed to start: \(error)")
    }
  }

  func seedFixtureIfNeeded() async {
    do {
      let sources = try await core.sources()
      // Ownership is keyed on the id we persisted when we seeded, never on the mutable display
      // name: a user source that happens to be named "Fixture Catalog" (e.g. a partial import from
      // the Phase 4 add-source flow) must never be treated as ours and torn down. The name is
      // re-verified only as a secondary guard, so a reused SQLite rowid (rowids are not
      // `AUTOINCREMENT`) can never authorize deleting a source we did not create.
      if let ownedId = Self.storedFixtureId,
        let fixture = sources.first(where: { $0.id == ownedId }),
        fixture.name == Self.fixtureSourceName
      {  // swiftlint:disable:this opening_brace
        let page = try await core.page(sourceId: fixture.id, offset: 0, limit: 1)
        if page.total > 0 { return }
        try await core.deleteSource(id: fixture.id)
        Self.storedFixtureId = nil
      } else if sources.isEmpty == false {
        return
      }
      let url = serveFixtureOnce(Self.fixturePlaylist())
      let source = try await core.addM3uUrl(name: Self.fixtureSourceName, url: url)
      Self.storedFixtureId = source.id
      for await event in core.importSource(id: source.id) {
        switch event {
        case .progress:
          continue
        case .complete(let outcome):
          logger.info("seeded \(outcome.inserted) channels")
        case .failed(let error):
          logger.error("fixture import failed: \(String(describing: error), privacy: .public)")
          try? await core.deleteSource(id: source.id)
          Self.storedFixtureId = nil
        }
      }
    } catch {
      logger.error("fixture seed failed: \(String(describing: error), privacy: .public)")
    }
  }

  /// The rowid of the fixture this app seeded, persisted across launches so ownership survives a
  /// later rename. Absent until the first successful seed; cleared when we tear the fixture down.
  private static var storedFixtureId: Int64? {
    get {
      let defaults = UserDefaults.standard
      guard defaults.object(forKey: fixtureIdKey) != nil else { return nil }
      return Int64(defaults.integer(forKey: fixtureIdKey))
    }
    set {
      let defaults = UserDefaults.standard
      if let newValue {
        defaults.set(Int(newValue), forKey: fixtureIdKey)
      } else {
        defaults.removeObject(forKey: fixtureIdKey)
      }
    }
  }

  private static func fixturePlaylist() -> [UInt8] {
    var text = "#EXTM3U\n"
    for index in 1...fixtureChannelCount {
      text += "#EXTINF:-1 tvg-id=\"ch\(index)\" group-title=\"Fixture\",Channel \(index)\n"
      text += "http://host.example/live/\(index).ts\n"
    }
    return Array(text.utf8)
  }

  private static let fixtureChannelCount = 24
  private static let fixtureSourceName = "Fixture Catalog"
  private static let fixtureIdKey = "dev.spidola.tv.fixtureSourceId"
  /// Bumped to 2 for the Phase 6 boundary: the typed settings vocabulary replaced the opaque
  /// get/set/remove surface, and Xtream and pairing arrived. This pin is the shell stating which
  /// boundary it was built against — the handshake guard above turns a mismatch into an immediate,
  /// legible stop rather than a puzzling failure later (TECH_SPEC §5), so it has to move in step
  /// with `core-api`'s `BOUNDARY_VERSION` or the app cannot launch at all.
  private static let supportedBoundaryVersion: UInt32 = 2
  private static let supportedSchemaVersion: UInt32 = 1
}

/// Serves `body` once over HTTP/1.1 from an ephemeral `127.0.0.1` port and returns its URL. Same
/// loopback pattern as the FFI contract harness; only used to seed the M0 fixture through the core.
private func serveFixtureOnce(_ body: [UInt8]) -> String {
  let fd = socket(AF_INET, SOCK_STREAM, 0)
  var yes: Int32 = 1
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
  var addr = sockaddr_in()
  addr.sin_family = sa_family_t(AF_INET)
  addr.sin_port = 0
  addr.sin_addr.s_addr = inet_addr("127.0.0.1")
  _ = withUnsafePointer(to: &addr) {
    $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
      bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
    }
  }
  _ = listen(fd, 1)

  var actual = sockaddr_in()
  var len = socklen_t(MemoryLayout<sockaddr_in>.size)
  _ = withUnsafeMutablePointer(to: &actual) {
    $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
      getsockname(fd, $0, &len)
    }
  }
  let port = UInt16(bigEndian: actual.sin_port)

  Thread.detachNewThread {
    let client = accept(fd, nil, nil)
    if client < 0 { return }
    var request = [UInt8](repeating: 0, count: 2048)
    _ = recv(client, &request, request.count, 0)
    let header =
      "HTTP/1.1 200 OK\r\nContent-Type: application/x-mpegurl\r\n"
      + "Content-Length: \(body.count)\r\nConnection: close\r\n\r\n"
    let headerBytes = Array(header.utf8)
    _ = headerBytes.withUnsafeBytes { send(client, $0.baseAddress, headerBytes.count, 0) }
    _ = body.withUnsafeBytes { send(client, $0.baseAddress, body.count, 0) }
    close(client)
    close(fd)
  }
  return "http://127.0.0.1:\(port)/fixture.m3u"
}
