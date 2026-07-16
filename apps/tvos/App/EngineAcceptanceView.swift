// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

#if DEBUG
  import PlayerAV
  import PlayerContract
  import PlayerMPV
  import SwiftUI

  /// Launch-environment configuration for the opt-in real-engine acceptance suite.
  ///
  /// This path exists only in debug builds. It never reads normal user state and never persists
  /// the supplied locator; the engine receives it through the same redacting StreamRequest used by
  /// normal playback.
  struct EngineAcceptanceConfiguration {
    let engine: String
    let locator: String

    static var current: Self? {
      let environment = ProcessInfo.processInfo.environment
      guard environment["SPIDOLA_ENGINE_ACCEPTANCE"] == "1",
        let engine = environment["SPIDOLA_ENGINE_ACCEPTANCE_ENGINE"],
        let locator = environment["SPIDOLA_ENGINE_ACCEPTANCE_LOCATOR"],
        !locator.isEmpty
      else { return nil }
      return Self(engine: engine, locator: locator)
    }
  }

  /// Hosts one real engine in the app process and exposes only its stable contract state to XCUITest.
  struct EngineAcceptanceView: View {
    let configuration: EngineAcceptanceConfiguration

    @State private var engine: (any PlaybackEngine)?
    @State private var state: PlaybackState = .idle

    var body: some View {
      ZStack {
        Color.black.ignoresSafeArea()
        if let engine {
          engine.makeSurface().ignoresSafeArea()
        }
        Text(state.acceptanceLabel)
          .foregroundStyle(.white)
          .accessibilityIdentifier("engine-acceptance-result")
          .accessibilityLabel(state.acceptanceLabel)
      }
      .task { await run() }
      .onDisappear(perform: stop)
    }

    private func run() async {
      guard engine == nil, let built = makeEngine() else {
        state = .failed(.unknown(detail: "unregistered acceptance engine"))
        return
      }
      engine = built
      built.load(StreamRequest(locator: configuration.locator))
      built.play()

      for await next in built.states {
        guard !Task.isCancelled else { return }
        state = next
        if next.isTerminal { return }
      }
    }

    private func makeEngine() -> (any PlaybackEngine)? {
      switch configuration.engine {
      case "mpv": MPVEngine()
      case "avplayer": AVPlayerEngine()
      default: nil
      }
    }

    private func stop() {
      engine?.stop()
      engine = nil
    }
  }

  extension PlaybackState {
    fileprivate var acceptanceLabel: String {
      switch self {
      case .idle: "idle"
      case .loading: "loading"
      case .buffering: "buffering"
      case .playing: "playing"
      case .paused: "paused"
      case .ended: "ended"
      case .failed(let error): "failed:\(error.acceptanceLabel)"
      }
    }
  }

  extension EngineError {
    fileprivate var acceptanceLabel: String {
      switch self {
      case .sourceUnreachable: "SourceUnreachable"
      case .unauthorized: "Unauthorized"
      case .unsupportedFormat: "UnsupportedFormat"
      case .decoderFailed: "DecoderFailed"
      case .timeout: "Timeout"
      case .unknown: "Unknown"
      }
    }
  }
#endif
