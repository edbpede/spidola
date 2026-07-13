// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Darwin
import Foundation
import OSLog

/// The composition root's single wiring point for the core (TECH_SPEC §3.1: composition happens
/// only at the app shell). Manual constructor wiring: the core is the one durable source of truth,
/// created once with the Keychain secrets store and OSLog sink installed here and nowhere else.
/// For the M0 walking skeleton it also seeds a fixture catalog through the core so browse has
/// content — mirroring the real add-source flow, which replaces it in Phase 4.
@MainActor
final class AppContainer {
  let core: SpidolaCore
  var catalog: any CatalogAccess { core }

  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::boot")

  init() {
    let dbPath = URL.documentsDirectory.appending(path: "spidola.sqlite").path()
    do {
      let core = try SpidolaCore(
        dbPath: dbPath,
        logDirectives: "info,spidola=debug",
        secrets: KeychainSecretStore(),
        logSink: OSLogSink()
      )
      let handshake = core.handshake()
      logger.info(
        "core \(handshake.coreVersion, privacy: .public), schema \(handshake.schemaVersion), boundary \(handshake.boundaryVersion)"
      )
      self.core = core
    } catch {
      // A failed boundary handshake is unrecoverable (TECH_SPEC §5): fail fast and legibly.
      fatalError("Spidola core failed to start: \(error)")
    }
  }

  func seedFixtureIfNeeded() async {
    do {
      guard try await core.sources().isEmpty else { return }
      let url = serveFixtureOnce(Self.fixturePlaylist())
      let source = try await core.addM3uUrl(name: "Fixture Catalog", url: url)
      for await event in core.importSource(id: source.id) {
        switch event {
        case .progress:
          continue
        case .complete(let outcome):
          logger.info("seeded \(outcome.inserted) channels")
        case .failed(let error):
          logger.error("fixture import failed: \(String(describing: error), privacy: .public)")
        }
      }
    } catch {
      logger.error("fixture seed failed: \(String(describing: error), privacy: .public)")
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
      "HTTP/1.1 200 OK\r\nContent-Type: application/x-mpegurl\r\nContent-Length: \(body.count)\r\nConnection: close\r\n\r\n"
    let headerBytes = Array(header.utf8)
    _ = headerBytes.withUnsafeBytes { send(client, $0.baseAddress, headerBytes.count, 0) }
    _ = body.withUnsafeBytes { send(client, $0.baseAddress, body.count, 0) }
    close(client)
    close(fd)
  }
  return "http://127.0.0.1:\(port)/fixture.m3u"
}
