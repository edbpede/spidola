// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreImage
import CoreImage.CIFilterBuiltins
import DesignSystem
import SwiftUI

/// Renders a URL as a QR code for a phone camera to read (PRD §6.1).
///
/// The QR is the shell's job — `core-pair` says so explicitly and holds no opinion about pixels —
/// and it costs no dependency: `CIQRCodeGenerator` is a CoreImage filter that has shipped in the
/// platform since tvOS 6.
///
/// Drawn light-on-dark against `set` rather than the palette's own colours: a QR code is read by a
/// camera, not a person, and cameras want maximum luminance contrast between modules. Test-Card
/// Amber is reserved for focus, the live indicator, and primary actions (PRD §8.2) — a decorative
/// amber QR would spend the app's one accent on a thing that is not any of those and would read
/// worse doing it.
struct QrCode: View {
  let text: String
  var side: CGFloat = 320

  var body: some View {
    Group {
      if let image {
        Image(decorative: image, scale: 1)
          // The generator emits roughly one pixel per module; `.none` keeps the modules crisp
          // squares when scaled up to TV size instead of smearing them into unreadable grey.
          .interpolation(.none)
          .resizable()
          .frame(width: side, height: side)
      } else {
        // The code failed to render but the URL and token beside it did not — this screen still
        // works by typing, so it says the picture is missing rather than pretending the whole
        // screen failed.
        Text(String(localized: "Can't draw the code — type the address instead.", bundle: .module))
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.studio)
          .multilineTextAlignment(.center)
          .frame(width: side, height: side)
      }
    }
    .padding(SpidolaSpacing.m)
    .background(.white)
    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    // A QR encodes the address shown beside it, so a screen reader has nothing to gain from the
    // picture and everything to lose from it being announced as an unlabelled image.
    .accessibilityHidden(true)
  }

  private var image: CGImage? {
    let filter = CIFilter.qrCodeGenerator()
    filter.message = Data(text.utf8)
    // Medium correction: a TV screen is a clean, flat, backlit surface photographed head-on, so
    // the higher levels would spend modules on damage resilience this code cannot suffer.
    filter.correctionLevel = "M"
    guard let output = filter.outputImage else { return nil }
    return CIContext().createCGImage(output, from: output.extent)
  }
}
