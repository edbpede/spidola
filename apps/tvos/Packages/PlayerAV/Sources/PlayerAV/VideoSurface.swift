// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import PlayerContract
import SwiftUI
import UIKit

extension AspectMode {
  /// The layer gravity this mode renders as.
  var videoGravity: AVLayerVideoGravity {
    switch self {
    case .fit: .resizeAspect
    case .fill: .resizeAspectFill
    case .stretch: .resize
    }
  }
}

/// The live state of the engine's video surface: the player to show, and how it fills the screen.
///
/// Separate from the engine so the SwiftUI view observes exactly the two things it draws. The
/// engine could have been `@Observable` itself, but then every `states` transition — several per
/// second while buffering — would invalidate a view whose appearance depends on neither.
@Observable
@MainActor
final class VideoSurface {
  let player: AVPlayer
  var gravity: AVLayerVideoGravity

  init(player: AVPlayer, gravity: AVLayerVideoGravity = AspectMode.fit.videoGravity) {
    self.player = player
    self.gravity = gravity
  }
}

/// A `UIView` whose backing layer *is* an `AVPlayerLayer`.
///
/// `layerClass` rather than a sublayer added at init: a sublayer has to be resized by hand on
/// every bounds change, and a missed frame there shows up as video that lags its own window
/// during the channel-strip transition. Making it the backing layer hands the geometry to UIKit,
/// which does not miss.
final class PlayerLayerView: UIView {
  override static var layerClass: AnyClass { AVPlayerLayer.self }

  /// The cast stays optional rather than forced. It cannot fail while `layerClass` above says
  /// `AVPlayerLayer` — but a `!` here would trade a black rectangle for a crash on the playback
  /// path, and the callers below have nothing to do with the result except set a property.
  private var playerLayer: AVPlayerLayer? { layer as? AVPlayerLayer }

  func attach(_ player: AVPlayer) {
    guard playerLayer?.player !== player else { return }
    playerLayer?.player = player
  }

  func apply(_ gravity: AVLayerVideoGravity) {
    guard playerLayer?.videoGravity != gravity else { return }
    playerLayer?.videoGravity = gravity
  }
}

/// Hosts the engine's `AVPlayerLayer` in SwiftUI.
///
/// Deliberately not `VideoPlayer`: that view brings tvOS's own transport controls, and Spidola
/// owns its playback UI — the info overlay, the track menus, and the channel strip in PRD §6.3
/// are all ours, and two transport layers competing for the same remote is not a thing to
/// negotiate. A bare layer renders video and nothing else.
private struct PlayerLayerSurface: UIViewRepresentable {
  let player: AVPlayer
  let gravity: AVLayerVideoGravity

  func makeUIView(context: Context) -> PlayerLayerView {
    let view = PlayerLayerView()
    view.backgroundColor = .black
    view.attach(player)
    view.apply(gravity)
    return view
  }

  func updateUIView(_ view: PlayerLayerView, context: Context) {
    view.attach(player)
    view.apply(gravity)
  }
}

/// The engine's `makeSurface()` view. Reading `surface.gravity` in `body` is what re-renders the
/// layer when the viewer cycles aspect.
struct VideoSurfaceView: View {
  let surface: VideoSurface

  var body: some View {
    PlayerLayerSurface(player: surface.player, gravity: surface.gravity)
      .ignoresSafeArea()
  }
}
