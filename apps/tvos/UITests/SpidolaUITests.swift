// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import XCTest

@MainActor
final class SpidolaUITests: XCTestCase {
  override func setUpWithError() throws {
    continueAfterFailure = false
  }

  /// Drives the Phase 4 drill-down on the seeded fixture catalog: home → source → category →
  /// channels, asserting D-pad focus lands and moves with the Test-Card Amber treatment.
  func testFixtureDrillDownAndDpadFocus() {
    let app = XCUIApplication()
    app.launchEnvironment["SPIDOLA_FIXTURE_UI_TEST"] = "1"
    app.launch()
    defer { app.terminate() }

    // Home: the fixture source is the first focusable element.
    let source = app.buttons["source-Fixture Catalog"]
    XCTAssertTrue(source.waitForExistence(timeout: 30))
    assertFocused(source)
    XCUIRemote.shared.press(.select)

    // Categories: the fixture playlist has one group.
    let group = app.buttons["group-Fixture"]
    XCTAssertTrue(group.waitForExistence(timeout: 10))
    XCUIRemote.shared.press(.select)

    // Channels: the first channel is focused; D-pad down moves to the second.
    let firstChannel = app.buttons["channel-Channel 1"]
    XCTAssertTrue(firstChannel.waitForExistence(timeout: 10))
    assertFocused(firstChannel)

    XCUIRemote.shared.press(.down)
    let secondChannel = app.buttons["channel-Channel 2"]
    XCTAssertTrue(secondChannel.waitForExistence(timeout: 5))
    assertFocused(secondChannel)

    let focusedState = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
    focusedState.name = "Channel 2 focused with Test-Card Amber treatment"
    focusedState.lifetime = .keepAlways
    add(focusedState)
  }

  func testColdLaunchQueuesDeepLinkUntilFixtureIsReady() throws {
    let app = XCUIApplication()
    app.launchEnvironment["SPIDOLA_FIXTURE_UI_TEST"] = "1"
    let url = try XCTUnwrap(URL(string: "spidola://search"))
    app.open(url)
    defer { app.terminate() }

    XCTAssertTrue(app.descendants(matching: .any)["search-field"].waitForExistence(timeout: 30))
  }

  private func assertFocused(
    _ element: XCUIElement,
    timeout: TimeInterval = 5,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let expectation = XCTNSPredicateExpectation(
      predicate: NSPredicate(format: "hasFocus == true"),
      object: element
    )
    XCTAssertEqual(
      XCTWaiter.wait(for: [expectation], timeout: timeout),
      .completed,
      "Expected element to receive tvOS focus",
      file: file,
      line: line
    )
  }
}
