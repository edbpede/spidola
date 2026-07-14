// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerMPV

/// mpv's `track-list` JSON mapped onto the contract's track menu.
///
/// The fixtures are shaped like mpv's real output, trimmed to the fields the mapping reads.
final class MPVTrackListTests: XCTestCase {

  func testParsesAudioAndSubtitleTracks() {
    let json = """
      [
        {"id":1,"type":"video","selected":true,"codec":"h264"},
        {"id":1,"type":"audio","title":"Commentary","lang":"eng","selected":true,"codec":"aac"},
        {"id":2,"type":"audio","lang":"deu","selected":false,"codec":"ac3"},
        {"id":1,"type":"sub","lang":"eng","selected":false,"codec":"subrip"}
      ]
      """
    let selection = MPVTrackList.parse(json: json)

    XCTAssertEqual(selection.tracks(of: .audio).count, 2)
    XCTAssertEqual(selection.tracks(of: .subtitle).count, 1)
    XCTAssertEqual(selection.selectedAudio, TrackID(rawValue: "1"))
    XCTAssertNil(selection.selectedSubtitle)
  }

  /// The contract has no video-track arm by its own design note, so those entries are dropped rather
  /// than forced into one of the two kinds that exist.
  func testVideoTracksAreDropped() {
    let json = """
      [{"id":1,"type":"video","selected":true,"codec":"hevc"}]
      """
    XCTAssertTrue(MPVTrackList.parse(json: json).available.isEmpty)
  }

  func testSelectedSubtitleIsReported() {
    let json = """
      [{"id":3,"type":"sub","lang":"fra","selected":true,"codec":"ass"}]
      """
    let selection = MPVTrackList.parse(json: json)
    XCTAssertEqual(selection.selectedSubtitle, TrackID(rawValue: "3"))
  }

  func testTitleIsPreferredAsLabel() {
    let json = """
      [{"id":1,"type":"audio","title":"Director's Cut","lang":"eng","codec":"aac"}]
      """
    XCTAssertEqual(MPVTrackList.parse(json: json).available.first?.label, "Director's Cut")
  }

  /// The common IPTV shape: a language and nothing else. The menu should read "eng", not
  /// "Audio 1".
  func testLanguageIsUsedWhenTitleIsAbsent() {
    let json = """
      [{"id":1,"type":"audio","lang":"eng","codec":"aac"}]
      """
    let track = MPVTrackList.parse(json: json).available.first
    XCTAssertEqual(track?.label, "eng")
    XCTAssertEqual(track?.language, "eng")
  }

  func testCodecIsUsedWhenTitleAndLanguageAreAbsent() {
    let json = """
      [{"id":1,"type":"audio","codec":"aac"}]
      """
    XCTAssertEqual(MPVTrackList.parse(json: json).available.first?.label, "AAC")
  }

  func testFallsBackToPositionalLabelWhenStreamDeclaresNothing() {
    let json = """
      [{"id":7,"type":"sub"}]
      """
    XCTAssertEqual(MPVTrackList.parse(json: json).available.first?.label, "Subtitle 7")
  }

  /// An unreadable track menu must not cost the viewer a channel that would otherwise play — the
  /// menu is a convenience, and the stream's default tracks still work without it.
  func testMalformedJSONYieldsEmptySelectionRatherThanFailing() {
    XCTAssertEqual(MPVTrackList.parse(json: "not json"), TrackSelection())
    XCTAssertEqual(MPVTrackList.parse(json: ""), TrackSelection())
    XCTAssertEqual(MPVTrackList.parse(json: "{}"), TrackSelection())
  }

  func testEmptyTrackListIsEmptySelection() {
    XCTAssertEqual(MPVTrackList.parse(json: "[]"), TrackSelection())
  }

  /// mpv's real entries carry many more fields than we read; decoding must ignore them rather than
  /// fail, or a libmpv upgrade that adds a field would empty every track menu.
  func testUnknownFieldsAreIgnored() {
    let json = """
      [{"id":1,"type":"audio","lang":"eng","selected":true,"codec":"aac",
        "src-id":2,"default":false,"forced":false,"external":false,
        "audio-channels":6,"demux-samplerate":48000,"ff-index":1}]
      """
    XCTAssertEqual(MPVTrackList.parse(json: json).selectedAudio, TrackID(rawValue: "1"))
  }

  /// mpv numbers audio and subtitle tracks independently, so ids collide across kinds. The menu must
  /// keep both, and selection must land on the right one — this is why `select` switches on the
  /// recorded kind rather than the id.
  func testAudioAndSubtitleIDsMayCollideAcrossKinds() {
    let json = """
      [{"id":1,"type":"audio","lang":"eng","selected":true},
       {"id":1,"type":"sub","lang":"eng","selected":true}]
      """
    let selection = MPVTrackList.parse(json: json)
    XCTAssertEqual(selection.available.count, 2)
    XCTAssertEqual(selection.selectedAudio, TrackID(rawValue: "1"))
    XCTAssertEqual(selection.selectedSubtitle, TrackID(rawValue: "1"))
  }
}
