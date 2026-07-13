// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The short, strict type scale from PRD §8.3 — display, title, body, caption — sized for the
/// tvOS 10-foot floor. Body and UI use the platform system face (SF Pro); the display face is a
/// characterful grotesque (Archivo, SIL OFL) bundled in a later slice — the scale below encodes
/// its weights and sizes now, falling back to the system face until the asset lands. Numerals are
/// tabular everywhere times or channel numbers appear.
public enum SpidolaType {
  public static let display = Font.system(size: 57, weight: .heavy).monospacedDigit()
  public static let title = Font.system(size: 38, weight: .bold).monospacedDigit()
  public static let body = Font.system(size: 29, weight: .regular).monospacedDigit()
  public static let caption = Font.system(size: 23, weight: .medium).monospacedDigit()
}
