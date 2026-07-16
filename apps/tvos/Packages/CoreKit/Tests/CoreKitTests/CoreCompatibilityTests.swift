// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import XCTest
import core_api

final class CoreCompatibilityTests: XCTestCase {
  func testCurrentShellAcceptsSchemaThreeBoundarySeven() throws {
    try CoreCompatibility.current.validate(Self.handshake(schema: 3, boundary: 7))
  }

  func testFrozenBoundaryFiveShellRejectsNewBoundary() {
    XCTAssertThrowsError(
      try CoreCompatibility.frozenBoundary5Shell.validate(Self.handshake(schema: 3, boundary: 7))
    ) { error in
      XCTAssertEqual(
        error as? CoreCompatibility.Rejection,
        .boundary(expected: 5, actual: 7))
    }
  }

  func testCurrentShellRejectsOlderBoundary() {
    XCTAssertThrowsError(
      try CoreCompatibility.current.validate(Self.handshake(schema: 3, boundary: 6))
    ) { error in
      XCTAssertEqual(
        error as? CoreCompatibility.Rejection,
        .boundary(expected: 7, actual: 6))
    }
  }

  private static func handshake(schema: UInt32, boundary: UInt32) -> Handshake {
    Handshake(
      coreVersion: "0.1.0", coreGitRevision: "test", schemaVersion: schema,
      boundaryVersion: boundary)
  }
}
