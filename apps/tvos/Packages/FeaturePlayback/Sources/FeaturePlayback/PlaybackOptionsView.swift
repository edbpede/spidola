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
        section("Audio") {
          ForEach(model.tracks.tracks(of: .audio)) { track in
            optionRow(
              title: label(for: track),
              isOn: model.tracks.selectedAudio == track.id
            ) { model.select(track: track.id) }
          }
          if model.tracks.tracks(of: .audio).isEmpty { emptyRow("Only one audio track") }
        }

        section("Subtitles") {
          optionRow(title: "Off", isOn: model.tracks.selectedSubtitle == nil) {
            model.clearSubtitle()
          }
          ForEach(model.tracks.tracks(of: .subtitle)) { track in
            optionRow(
              title: label(for: track),
              isOn: model.tracks.selectedSubtitle == track.id
            ) { model.select(track: track.id) }
          }
        }

        section("Picture") {
          optionRow(title: model.aspect.label, isOn: false) { model.cycleAspect() }
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
      content()
    }
  }

  private func optionRow(
    title: String, isOn: Bool, action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack {
        Text(title)
        Spacer(minLength: 0)
        if isOn {
          Image(systemName: "checkmark")
            .foregroundStyle(SpidolaPalette.testCardAmber)
        }
      }
    }
    .font(SpidolaType.body)
  }

  private func emptyRow(_ text: String) -> some View {
    Text(text)
      .font(SpidolaType.body)
      .foregroundStyle(SpidolaPalette.staticGray)
  }

  /// Prefers the stream's language tag, since "English" beats "Track 2" from the couch.
  private func label(for track: MediaTrack) -> String {
    if let language = track.language, !language.isEmpty {
      return track.label.isEmpty ? language : "\(track.label) · \(language)"
    }
    return track.label
  }

  private static let panelWidth: CGFloat = 640
}
