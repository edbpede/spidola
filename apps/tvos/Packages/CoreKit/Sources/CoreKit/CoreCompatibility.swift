// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// The shell's explicit compatibility contract with the generated core boundary and database.
public struct CoreCompatibility: Sendable, Equatable {
  public enum Rejection: Error, Sendable, Equatable {
    case missingCoreVersion
    case schema(expected: UInt32, actual: UInt32)
    case boundary(expected: UInt32, actual: UInt32)
  }

  public let schemaVersion: UInt32
  public let boundaryVersion: UInt32

  public init(schemaVersion: UInt32, boundaryVersion: UInt32) {
    self.schemaVersion = schemaVersion
    self.boundaryVersion = boundaryVersion
  }

  public static let current = CoreCompatibility(schemaVersion: 3, boundaryVersion: 7)
  public static let frozenBoundary5Shell = CoreCompatibility(schemaVersion: 3, boundaryVersion: 5)

  public func validate(_ handshake: Handshake) throws {
    guard !handshake.coreVersion.isEmpty else { throw Rejection.missingCoreVersion }
    guard handshake.schemaVersion == schemaVersion else {
      throw Rejection.schema(expected: schemaVersion, actual: handshake.schemaVersion)
    }
    guard handshake.boundaryVersion == boundaryVersion else {
      throw Rejection.boundary(expected: boundaryVersion, actual: handshake.boundaryVersion)
    }
  }
}
