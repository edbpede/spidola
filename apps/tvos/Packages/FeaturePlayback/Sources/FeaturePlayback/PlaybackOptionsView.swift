// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import PlayerContract
import SwiftUI

/// Playback options, summoned by the menu button (PRD §8.4): audio, subtitles, and aspect.
///
/// The vocabulary is engine-neutral by construction — every option here routes through the
/// contract, so the viewer never learns which decoder is running (TECH_SPEC §8). Engine choice
/// itself is not offered here: it is a per-channel decision the loud fallback already asks about
/// at the only moment it means anything, and a menu entry would invite fiddling with something the
/// app is supposed to get right on its own.
struct PlaybackOptionsView: View {
  let model: PlaybackModel
  let onClose: () -> Void

  var body: some View {
    HStack {
      Spacer()
      VStack(alignment: .leading, spacing: SpidolaSpacing.l) {
        section(String(localized: "Audio", bundle: .module)) {
          ForEach(model.tracks.tracks(of: .audio)) { track in
            optionRow(
              title: label(for: track),
              isOn: model.tracks.selectedAudio == track.id
            ) { model.select(track: track.id) }
          }
          if model.tracks.tracks(of: .audio).isEmpty {
            emptyRow(String(localized: "Only one audio track", bundle: .module))
          }
        }

        section(String(localized: "Subtitles", bundle: .module)) {
          optionRow(
            title: String(localized: "Off", bundle: .module),
            isOn: model.tracks.selectedSubtitle == nil
          ) {
            model.clearSubtitle()
          }
          ForEach(model.tracks.tracks(of: .subtitle)) { track in
            optionRow(
              title: label(for: track),
              isOn: model.tracks.selectedSubtitle == track.id
            ) { model.select(track: track.id) }
          }
        }

        section(String(localized: "Picture", bundle: .module)) {
          optionRow(title: model.aspect.localizedLabel, isOn: nil) { model.cycleAspect() }
        }
      }
      .padding(SpidolaSpacing.xl)
      .frame(width: Self.panelWidth, alignment: .leading)
      .background(SpidolaPalette.set)
    }
    .ignoresSafeArea()
    .onExitCommand(perform: onClose)
  }

  private func section(
    _ title: String, @ViewBuilder content: () -> some View
  ) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text(title)
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
        // The header names the group its rows belong to, so a listener can skim the panel the way
        // a viewer skims it rather than hearing every track before learning it was the audio list.
        .accessibilityAddTraits(.isHeader)
      content()
    }
  }

  /// `isOn` is `nil` for a row that cycles rather than picks — the aspect row, whose title is
  /// already the value in force. "Not selected" there would describe a choice the row does not
  /// offer.
  private func optionRow(
    title: String, isOn: Bool?, action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack {
        Text(title)
        Spacer(minLength: 0)
        if isOn == true {
          Image(systemName: "checkmark")
            .foregroundStyle(SpidolaPalette.testCardAmber)
        }
      }
    }
    .font(SpidolaType.body)
    // The checkmark is the only thing marking the track in force, and a glyph has no voice: a
    // listener would hear every track read out identically and none of them as the one playing.
    .accessibilityLabel(title)
    .accessibilityValue(selection(isOn))
  }

  private func selection(_ isOn: Bool?) -> String {
    guard let isOn else { return "" }
    return isOn
      ? String(localized: "Selected", bundle: .module)
      : String(localized: "Not selected", bundle: .module)
  }

  private func emptyRow(_ text: String) -> some View {
    Text(text)
      .font(SpidolaType.body)
      .foregroundStyle(SpidolaPalette.staticGray)
  }

  /// Prefers the stream's language tag, since "English" beats "Track 2" from the couch.
  ///
  /// An engine reports an empty label for a track the stream named in no way at all, leaving this
  /// slice — the one with a catalog — to number it. Falling through to a blank row would offer the
  /// viewer a track they cannot tell from the one above it.
  private func label(for track: MediaTrack) -> String {
    if let language = track.language, !language.isEmpty {
      return track.label.isEmpty ? language : "\(track.label) · \(language)"
    }
    guard track.label.isEmpty else { return track.label }
    return track.localizedFallbackLabel(position: position(of: track))
  }

  /// The track's place in the menu it is being shown in, counted from one, which is the number the
  /// viewer is looking at.
  private func position(of track: MediaTrack) -> Int {
    let siblings = model.tracks.tracks(of: track.kind)
    return (siblings.firstIndex { $0.id == track.id } ?? 0) + 1
  }

  private static let panelWidth: CGFloat = 640
}
