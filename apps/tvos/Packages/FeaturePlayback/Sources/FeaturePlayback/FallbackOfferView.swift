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
        Button(String(localized: "Try other player", bundle: .module)) { onTry(remember) }
          .focused($focused, equals: .tryOther)
        Button(
          remember
            ? String(localized: "Remember for this channel", bundle: .module)
            : String(localized: "Just this once", bundle: .module)
        ) { remember.toggle() }
        .focused($focused, equals: .remember)
        // Alone among these three, this button's title states the choice in force rather than what
        // pressing it does — which a viewer reads from its place beside "Try other player", and a
        // listener arriving at it cold cannot. The hint is where that goes.
        .accessibilityHint(
          String(localized: "Changes whether this choice sticks for this channel.", bundle: .module)
        )
        Button(String(localized: "Go back", bundle: .module)) { onBack() }
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

  // These two titles are deliberately not localized, alone in this file. They are not this
  // slice's words: they are the `ErrorAction` vocabulary spelled out by hand, and the class and
  // message above them come from `EngineError` in the same English. Resourcing the buttons would
  // translate the answer and leave the question — the half-done state that reads as finished,
  // which is the whole reason that vocabulary is excluded rather than swept (TECH_SPEC §14). It
  // goes when the core returns a code and the shells own the words; until then it goes nowhere.
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
      Text(String(localized: "This channel is live — there's nothing to skip to.", bundle: .module))
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
