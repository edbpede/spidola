// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import XCTest
import core_api

final class ActionableErrorTests: XCTestCase {
  func testStructuredInvalidInputUsesShellCopy() {
    let error = ActionableError(.InvalidInput(field: .address, issue: .invalid))

    XCTAssertEqual(error.failureClass, "That entry isn't valid")
    XCTAssertEqual(error.message, "Check the address and try again.")
    XCTAssertEqual(error.primaryAction, .fixInput)
  }

  func testEveryBoundaryErrorHasAnAction() {
    let errors: [ApiError] = [
      .NetworkUnreachable, .Timeout, .Unauthorized, .NotFound,
      .InvalidInput(field: .name, issue: .empty), .ParseFailed(emitted: 1, skipped: 2),
      .StorageCorrupt, .Cancelled, .Internal,
    ]

    XCTAssertTrue(errors.allSatisfy { !ActionableError($0).actions.isEmpty })
  }
}
