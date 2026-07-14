// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerMPV

/// The log-redaction invariant (TECH_SPEC §12: secret values never appear in interpolated log
/// messages).
///
/// This suite is the invariant's executable form. The rule is easy to state and easy to break by
/// adding one convenient interpolation, so the cases below are written as "this string must not
/// appear in the output" rather than "the output equals X" — they fail on a leak regardless of how
/// the summary is reformatted.
final class MPVRedactionTests: XCTestCase {

  /// The case that motivates the whole file: an Xtream locator carries the account in its **path**,
  /// so logging a URL's path logs the viewer's username and password.
  func testXtreamCredentialsInPathAreNotLogged() {
    let locator = "http://line.example.com:8080/live/joeblogs/s3cr3tpassw0rd/12345.ts"
    let summary = MPVRedaction.locatorSummary(locator)

    XCTAssertFalse(summary.contains("joeblogs"))
    XCTAssertFalse(summary.contains("s3cr3tpassw0rd"))
    XCTAssertEqual(summary, "http://line.example.com:8080")
  }

  func testURLUserInfoIsNotLogged() {
    let summary = MPVRedaction.locatorSummary("http://user:hunter2@example.com/stream.m3u8")
    XCTAssertFalse(summary.contains("user"))
    XCTAssertFalse(summary.contains("hunter2"))
    XCTAssertEqual(summary, "http://example.com")
  }

  func testQueryTokensAreNotLogged() {
    let summary = MPVRedaction.locatorSummary(
      "https://cdn.example.com/live.m3u8?token=abc123&auth=deadbeef")
    XCTAssertFalse(summary.contains("abc123"))
    XCTAssertFalse(summary.contains("deadbeef"))
    XCTAssertFalse(summary.contains("token"))
    XCTAssertEqual(summary, "https://cdn.example.com")
  }

  /// Host and port survive on purpose: they are what make a `sourceUnreachable` report actionable,
  /// and neither is a credential.
  func testHostAndPortSurviveBecauseTheyDiagnoseWithoutIdentifying() {
    XCTAssertEqual(
      MPVRedaction.locatorSummary("https://cdn.example.com:8443/x/y"),
      "https://cdn.example.com:8443")
    XCTAssertEqual(
      MPVRedaction.locatorSummary("https://cdn.example.com/x"), "https://cdn.example.com")
  }

  func testUnparsableLocatorDoesNotEchoItself() {
    let summary = MPVRedaction.locatorSummary("://not a url at all/secret")
    XCTAssertFalse(summary.contains("secret"))
    XCTAssertEqual(summary, "<unparsable locator>")
  }

  /// A header override exists precisely to carry a token, so the value is assumed secret with no
  /// exception. The names alone answer the only question the log needs to settle.
  func testHeaderValuesAreNeverLoggedButNamesAre() {
    let headers = [
      StreamHeader(name: "Authorization", value: "Bearer super-secret-token"),
      StreamHeader(name: "X-Session", value: "abcdef123456"),
    ]
    let rendered = MPVRedaction.headerNames(headers)

    XCTAssertFalse(rendered.contains("super-secret-token"))
    XCTAssertFalse(rendered.contains("Bearer"))
    XCTAssertFalse(rendered.contains("abcdef123456"))
    XCTAssertEqual(rendered, "Authorization, X-Session")
  }

  func testNoHeadersRendersLegibly() {
    XCTAssertEqual(MPVRedaction.headerNames([]), "<none>")
  }

  /// A user-agent override is a fingerprint a source hands out to identify an account, so only its
  /// presence is reportable.
  func testUserAgentValueIsNeverLogged() {
    let rendered = MPVRedaction.userAgentPresence("SecretPlayer/1.0 (token=abc123)")
    XCTAssertFalse(rendered.contains("abc123"))
    XCTAssertFalse(rendered.contains("SecretPlayer"))
    XCTAssertEqual(rendered, "overridden")
    XCTAssertEqual(MPVRedaction.userAgentPresence(nil), "default")
  }
}
