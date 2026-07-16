// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import PlayerContract
import SwiftUI
import core_api

/// The playback screen: engine surface, the channel strip, and the remote mapping from PRD §8.4.
///
/// Everything here is quiet so the strip can sing (PRD §8.1). The screen has no chrome of its own:
/// video fills it, and every control is summoned.
public struct PlaybackView: View {
  @State private var model: PlaybackModel
  @State private var strip = StripPresenter()
  @State private var isShowingOptions = false
  @State private var seekHint = false

  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private let onExit: () -> Void

  public init(
    channel: PlayableChannel,
    context: ZapContext,
    offset: UInt32,
    access: any PlaybackAccess & EpgAccess,
    registry: EngineRegistry,
    onExit: @escaping () -> Void
  ) {
    _model = State(
      initialValue: PlaybackModel(
        channel: channel, context: context, offset: offset, access: access, registry: registry))
    self.onExit = onExit
  }

  public init(
    customChannel: CustomPlayableChannel,
    access: any PlaybackAccess & EpgAccess,
    registry: EngineRegistry,
    onExit: @escaping () -> Void
  ) {
    _model = State(
      initialValue: PlaybackModel(
        customChannel: customChannel, access: access, registry: registry))
    self.onExit = onExit
  }

  public var body: some View {
    ZStack {
      SpidolaPalette.studio.ignoresSafeArea()
      if let engine = model.engine {
        engine.makeSurface().ignoresSafeArea()
      }
      loadingTreatment
      overlays
    }
    .focusable()
    .onMoveCommand(perform: move)
    .onPlayPauseCommand { model.togglePause() }
    .onExitCommand(perform: exit)
    // Select summons the strip (PRD §8.4). tvOS routes the remote's select to a tap on the
    // focused view.
    .onTapGesture { strip.summon() }
    .task { await model.start() }
    .onDisappear { model.stop() }
  }

  /// Shown only while there is no video. The strip and the error surfaces own everything after.
  @ViewBuilder private var loadingTreatment: some View {
    if !model.state.isShowingVideo && model.fallbackOffer == nil && model.state.failure == nil {
      VStack(spacing: SpidolaSpacing.m) {
        ProgressView().tint(SpidolaPalette.testCardAmber)
        Text(model.displayName)
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
      }
    }
  }

  @ViewBuilder private var overlays: some View {
    if let offer = model.fallbackOffer {
      FallbackOfferView(
        offer: offer,
        canRemember: model.canRememberEngine,
        onTry: { remember in Task { await model.tryOtherPlayer(remember: remember) } },
        onDismiss: { model.dismissFallback() },
        onBack: exit)
    } else if let error = model.state.failure {
      // A failure with no other engine to offer still has to say what happened and what to press
      // (PRD §6.3) — an error with no action is a design bug.
      PlaybackErrorView(error: error, onRetry: { Task { await model.start() } }, onBack: exit)
    } else if isShowingOptions {
      PlaybackOptionsView(model: model, onClose: { isShowingOptions = false })
    } else if strip.isVisible {
      VStack {
        Spacer()
        if let channel = model.channel {
          ChannelStrip(
            window: model.window, channel: channel, isLive: model.isLive,
            nowNext: model.nowNext)
        } else if let customChannel = model.customChannel {
          CustomChannelStrip(channel: customChannel)
        }
        Spacer().frame(height: SpidolaSpacing.safeVertical)
      }
      .transition(stripTransition)
      if seekHint { SeekHintView() }
    }
  }

  /// The strip slides up (PRD §8.5), under 200 ms and suppressed under reduce-motion (§8.6).
  private var stripTransition: AnyTransition {
    reduceMotion
      ? .opacity
      : .move(edge: .bottom).combined(with: .opacity)
  }

  private func move(_ direction: MoveCommandDirection) {
    switch direction {
    case .up:
      zap(.previous)
    case .down:
      zap(.next)
    case .left:
      seek(by: -Self.seekStep)
    case .right:
      seek(by: Self.seekStep)
    @unknown default:
      break
    }
  }

  private func zap(_ direction: ZapDirection) {
    // The strip rides the zap: a viewer flipping channels wants to see what they landed on.
    strip.summon()
    Task { await model.zap(direction) }
  }

  private func seek(by seconds: Double) {
    guard model.isSeekable else {
      // "No-op with hint" (PRD §8.4) — a live stream cannot seek, and silence would read as a
      // broken remote.
      showSeekHint()
      return
    }
    model.seek(by: seconds)
    strip.summon()
  }

  private func showSeekHint() {
    seekHint = true
    strip.summon()
    Task {
      try? await Task.sleep(for: .seconds(2))
      seekHint = false
    }
  }

  /// Back dismisses an overlay first, and only then leaves (PRD §8.4).
  private func exit() {
    if isShowingOptions {
      isShowingOptions = false
    } else if model.fallbackOffer != nil {
      model.dismissFallback()
    } else if strip.isVisible {
      strip.dismiss()
    } else {
      model.stop()
      onExit()
    }
  }

  private static let seekStep: Double = 10
}

/// Owns the strip's visibility and its self-dismiss timer.
///
/// Its own type because "summon, then dismiss unless summoned again" is a small state machine, and
/// leaving it inline in the view would put a cancellable task in a struct that is recreated on
/// every render.
@MainActor
@Observable
final class StripPresenter {
  private(set) var isVisible = false
  private var timeout: Task<Void, Never>?

  /// Shows the strip and restarts its timer. Re-summoning while visible extends it, so a viewer
  /// zapping steadily never has the strip vanish mid-flip.
  func summon() {
    isVisible = true
    timeout?.cancel()
    timeout = Task { [weak self] in
      try? await Task.sleep(for: .seconds(Self.dwell))
      guard !Task.isCancelled else { return }
      self?.isVisible = false
    }
  }

  func dismiss() {
    timeout?.cancel()
    timeout = nil
    isVisible = false
  }

  /// Long enough to read a channel name and glance at the neighbours; short enough that it never
  /// feels like chrome the viewer has to dismiss.
  private static let dwell: Double = 5
}

extension PlayableChannel {
  /// The live marker is only honest for live channels; a movie has no "LIVE".
  var isLive: Bool { kind == .live }
}
