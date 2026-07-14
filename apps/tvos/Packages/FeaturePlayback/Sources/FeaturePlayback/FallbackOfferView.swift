// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import PlayerContract
import SwiftUI

/// The loud fallback (TECH_SPEC §8): the engine failed in a way another engine could plausibly
/// survive, so the viewer is *offered* the swap and chooses. Nothing switches on its own.
///
/// The remember toggle is the difference between a one-off rescue and a channel that simply works
/// from now on — a channel whose format only one engine handles is a permanent fact about that
/// channel, and making the viewer re-answer nightly would be the bug.
struct FallbackOfferView: View {
  let offer: FallbackOffer
  let onTry: (Bool) -> Void
  let onDismiss: () -> Void
  let onBack: () -> Void

  @State private var remember = true
  @FocusState private var focused: Field?

  private enum Field: Hashable { case tryOther, remember, back }

  var body: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      Text(offer.error.failureClass)
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      Text(offer.error.message)
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.staticGray)

      HStack(spacing: SpidolaSpacing.m) {
        Button("Try other player") { onTry(remember) }
          .focused($focused, equals: .tryOther)
        Button(remember ? "Remember for this channel" : "Just this once") { remember.toggle() }
          .focused($focused, equals: .remember)
        Button("Go back") { onBack() }
          .focused($focused, equals: .back)
      }
      .font(SpidolaType.body)
      .padding(.top, SpidolaSpacing.s)
    }
    .padding(SpidolaSpacing.xl)
    .background(SpidolaPalette.set)
    .clipShape(RoundedRectangle(cornerRadius: SpidolaSpacing.m))
    .padding(SpidolaSpacing.xl)
    .onAppear { focused = .tryOther }
    .onExitCommand(perform: onDismiss)
  }
}

/// A playback failure with no other engine to offer. Still says what happened and what to press
/// next — an error with no action is a design bug (PRD §6.3).
struct PlaybackErrorView: View {
  let error: EngineError
  let onRetry: @MainActor () -> Void
  let onBack: @MainActor () -> Void

  var body: some View {
    ActionableErrorView(
      failureClass: error.failureClass,
      message: error.message,
      primary: SpidolaErrorButton(title: "Try again", action: onRetry),
      others: [SpidolaErrorButton(title: "Go back", action: onBack)])
  }
}

/// Shown when left/right is pressed on a stream that cannot seek (PRD §8.4: "no-op with hint").
/// Silence would read as a broken remote.
struct SeekHintView: View {
  var body: some View {
    VStack {
      Spacer()
      Text("This channel is live — there's nothing to skip to.")
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
        .padding(.horizontal, SpidolaSpacing.m)
        .padding(.vertical, SpidolaSpacing.s)
        .background(SpidolaPalette.set.opacity(0.9))
        .clipShape(Capsule())
      Spacer().frame(height: SpidolaSpacing.safeVertical)
    }
    .accessibilityAddTraits(.isStaticText)
  }
}
