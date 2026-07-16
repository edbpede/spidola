// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

@MainActor
@Observable
public final class CustomSharingModel {
  public var exportContents = ""
  public var importContents = ""
  public private(set) var status: String?

  private let access: any CustomChannelsAccess

  public init(access: any CustomChannelsAccess) {
    self.access = access
  }

  public func prepareExport() async {
    do {
      exportContents = try await access.exportCustomChannels()
      status = String(localized: "Export ready.", bundle: .module)
    } catch {
      status = ActionableError((error as? ApiError) ?? .Internal).message
    }
  }

  public func importChannels(mode: CustomImportMode) async {
    do {
      let imported = try await access.importCustomChannels(importContents, mode: mode)
      status = String(
        localized: "Imported \(imported) channels.", bundle: .module,
        comment: "Portable custom-channel import result.")
      importContents = ""
    } catch {
      status = ActionableError((error as? ApiError) ?? .Internal).message
    }
  }
}
