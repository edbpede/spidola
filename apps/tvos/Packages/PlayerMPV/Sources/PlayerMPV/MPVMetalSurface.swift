// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import QuartzCore
import SwiftUI
import UIKit

/// The `CAMetalLayer` mpv renders into.
///
/// **How the video actually reaches the screen:** libmpv's render API (`mpv_render_context_create`)
/// offers only OpenGL and a software renderer — there is no Metal render API. OpenGL ES is
/// deprecated on tvOS and cannot decode 10-bit correctly (mpv#7846), and the software renderer is
/// not a serious option for a TV. So this engine takes MPVKit's supported path instead: mpv is
/// pointed at this layer through its `wid` option and drives it itself with
/// `vo=gpu-next, gpu-api=vulkan, gpu-context=moltenvk`, where MoltenVK translates Vulkan onto
/// Metal. The pixels are Metal end to end; mpv owns the swapchain rather than us.
///
/// That path exists because MPVKit patches a MoltenVK context into libmpv (mpv#7857, still not
/// upstream). It is the route MPVKit's own tvOS demo takes and the reason the `MoltenVK`
/// xcframework is in the dependency graph at all — recorded here because "why not the render API?"
/// is the first question a reader will have.
final class MPVMetalLayer: CAMetalLayer {
  /// Rejects the degenerate size MoltenVK sets while forcing a presentation to complete.
  ///
  /// Without this the layer can be left at 1x1 and the picture flickers or disappears (mpv#13651).
  /// The workaround is upstream MPVKit's, kept here because it is load-bearing rather than
  /// cosmetic: the layer never carries a size no real display would have.
  override var drawableSize: CGSize {
    get { super.drawableSize }
    set {
      guard newValue.width > 1, newValue.height > 1 else { return }
      super.drawableSize = newValue
    }
  }
}

/// The `UIView` hosting the render layer.
///
/// The layer is created and owned by `MPVEngine`, not by this view: mpv needs its address before
/// `mpv_initialize`, and the layer must survive any particular hosting view SwiftUI happens to
/// build or rebuild. It outlives this view at teardown too, and by design — the view goes away with
/// the rest of the tree the moment the model drops the engine, while `MPVCoreDisposal` holds the
/// layer up for the core it is still destroying.
/// The view's whole job is keeping the layer's geometry in step with the frame it is given.
final class MPVSurfaceView: UIView {
  private let metalLayer: MPVMetalLayer

  init(metalLayer: MPVMetalLayer) {
    self.metalLayer = metalLayer
    super.init(frame: .zero)
    backgroundColor = .black
    layer.addSublayer(metalLayer)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    // This view is only ever built in code; there is no storyboard path to it. Trapping here is
    // the honest answer for an initialiser that cannot be reached rather than one that can fail.
    fatalError("MPVSurfaceView is not available from a coder")
  }

  override func layoutSubviews() {
    super.layoutSubviews()

    // Layer geometry changes animate implicitly. During a resize that shows as the video sliding
    // into place a beat behind the layout, so the implicit animation is disabled rather than
    // fought with a duration.
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    metalLayer.frame = bounds
    metalLayer.contentsScale = traitCollection.displayScale
    CATransaction.commit()
  }
}

/// The SwiftUI surface the playback screen hosts, per `PlaybackEngine.makeSurface`.
struct MPVMetalSurface: UIViewRepresentable {
  let metalLayer: MPVMetalLayer

  func makeUIView(context: Context) -> MPVSurfaceView {
    MPVSurfaceView(metalLayer: metalLayer)
  }

  func updateUIView(_ uiView: MPVSurfaceView, context: Context) {
    // Nothing to push: the engine drives the layer through mpv, and SwiftUI drives the view's
    // frame. There is no state flowing from the view tree into the surface.
  }
}
