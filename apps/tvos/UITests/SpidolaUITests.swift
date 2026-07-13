// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import XCTest

@MainActor
final class SpidolaUITests: XCTestCase {
  override func setUpWithError() throws {
    continueAfterFailure = false
  }

  func testFixtureCatalogAndDpadFocus() {
    let app = XCUIApplication()
    app.launch()

    let firstChannel = app.buttons["channel-Channel 1"]
    XCTAssertTrue(firstChannel.waitForExistence(timeout: 30))
    XCTAssertTrue(firstChannel.hasFocus)

    XCUIRemote.shared.press(.down)

    let secondChannel = app.buttons["channel-Channel 2"]
    XCTAssertTrue(secondChannel.waitForExistence(timeout: 5))
    XCTAssertTrue(secondChannel.hasFocus)

    let focusedState = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
    focusedState.name = "Channel 2 focused with Test-Card Amber treatment"
    focusedState.lifetime = .keepAlways
    add(focusedState)
  }
}
