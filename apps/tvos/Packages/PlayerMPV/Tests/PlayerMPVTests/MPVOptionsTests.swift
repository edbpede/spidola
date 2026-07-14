// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerMPV

/// The contract's engine-neutral vocabulary mapped onto mpv's knobs (TECH_SPEC §8).
///
/// These assert the *relationships* the profiles promise rather than the exact numbers. The numbers
/// are admittedly unmeasured starting points (see `MPVOptions.cacheOptions`), so pinning them here
/// would make the suite a change-detector that fails the moment someone tunes them against a real
/// headend — which is the thing we want to happen. What must not change without a deliberate
/// decision is the ordering: `.low` starts faster, `.generous` buffers more.
final class MPVOptionsTests: XCTestCase {

  private func value(_ options: [MPVOption], _ name: String) -> String? {
    options.first { $0.name == name }?.value
  }

  private func cacheSecs(_ profile: BufferingProfile) -> Double {
    Double(value(MPVOptions.cacheOptions(for: profile), "cache-secs") ?? "") ?? -1
  }

  private func readahead(_ profile: BufferingProfile) -> Double {
    Double(value(MPVOptions.cacheOptions(for: profile), "demuxer-readahead-secs") ?? "") ?? -1
  }

  // MARK: - Buffering

  func testEveryProfileEnablesTheCache() {
    for profile in BufferingProfile.allCases {
      XCTAssertEqual(value(MPVOptions.cacheOptions(for: profile), "cache"), "yes", "\(profile)")
    }
  }

  /// The profile exists to trade latency against resilience. If the ordering ever inverted, the
  /// setting would still "work" while doing the opposite of what its label promises.
  func testBufferSizeIncreasesWithResilience() {
    XCTAssertLessThan(cacheSecs(.low), cacheSecs(.balanced))
    XCTAssertLessThan(cacheSecs(.balanced), cacheSecs(.generous))
  }

  func testReadaheadIncreasesWithResilience() {
    XCTAssertLessThan(readahead(.low), readahead(.balanced))
    XCTAssertLessThan(readahead(.balanced), readahead(.generous))
  }

  /// `cache-pause-initial` decides whether the first frame waits for a full buffer, so it is the
  /// option that can blow the two-second zap budget (PRD §9). Only `.generous`, where the viewer
  /// explicitly asked for smoothness over speed, may turn it on.
  func testOnlyGenerousWaitsForTheBufferBeforeTheFirstFrame() {
    XCTAssertEqual(value(MPVOptions.cacheOptions(for: .low), "cache-pause-initial"), "no")
    XCTAssertEqual(value(MPVOptions.cacheOptions(for: .balanced), "cache-pause-initial"), "no")
    XCTAssertEqual(value(MPVOptions.cacheOptions(for: .generous), "cache-pause-initial"), "yes")
  }

  // MARK: - Aspect

  /// Each mode must set *both* knobs. Leaving one unset would let the previous mode's value survive
  /// the cycle, so `.fit` after `.fill` would stay cropped.
  func testEveryAspectModeSetsBothKnobs() {
    for mode in AspectMode.allCases {
      let options = MPVOptions.aspectOptions(for: mode)
      XCTAssertNotNil(value(options, "keepaspect"), "\(mode)")
      XCTAssertNotNil(value(options, "panscan"), "\(mode)")
    }
  }

  func testFitKeepsAspectAndDoesNotCrop() {
    let options = MPVOptions.aspectOptions(for: .fit)
    XCTAssertEqual(value(options, "keepaspect"), "yes")
    XCTAssertEqual(value(options, "panscan"), "0")
  }

  func testFillKeepsAspectAndCropsFully() {
    let options = MPVOptions.aspectOptions(for: .fill)
    XCTAssertEqual(value(options, "keepaspect"), "yes")
    XCTAssertEqual(value(options, "panscan"), "1.0")
  }

  /// Stretch abandons aspect; panscan has nothing to crop and is reset so a later `.fill` starts
  /// from a known state.
  func testStretchAbandonsAspectAndResetsCrop() {
    let options = MPVOptions.aspectOptions(for: .stretch)
    XCTAssertEqual(value(options, "keepaspect"), "no")
    XCTAssertEqual(value(options, "panscan"), "0")
  }

  /// The UI cycles aspect with a single button, so every mode in the cycle must produce a distinct
  /// geometry — two modes rendering identically would read as a dead button press.
  func testAspectCycleProducesDistinctGeometries() {
    let rendered = AspectMode.allCases.map { MPVOptions.aspectOptions(for: $0) }
    XCTAssertEqual(Set(rendered.map { "\($0)" }).count, AspectMode.allCases.count)
  }

  // MARK: - Headers

  func testHeadersRenderInMPVWireForm() {
    let headers = [
      StreamHeader(name: "Authorization", value: "Bearer xyz"),
      StreamHeader(name: "Referer", value: "https://example.com/"),
    ]
    XCTAssertEqual(
      MPVOptions.headerFields(headers),
      ["Authorization: Bearer xyz", "Referer: https://example.com/"])
  }

  /// The reason these go to mpv as a node array rather than a comma-joined string: a value with a
  /// comma must survive intact, and mpv's option parser would split it into two bogus headers.
  func testHeaderValuesContainingCommasArePreservedWhole() {
    let headers = [StreamHeader(name: "Accept", value: "text/html,application/xml")]
    XCTAssertEqual(MPVOptions.headerFields(headers), ["Accept: text/html,application/xml"])
  }

  func testNoHeadersRendersEmpty() {
    XCTAssertTrue(MPVOptions.headerFields([]).isEmpty)
  }
}
