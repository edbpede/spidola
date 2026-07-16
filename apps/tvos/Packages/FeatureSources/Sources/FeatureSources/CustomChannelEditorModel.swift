// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import Observation
import core_api

@MainActor
@Observable
public final class CustomChannelEditorModel {
  public var input: CustomChannelInput
  public private(set) var validationMessage: String?
  public private(set) var isSaving = false

  private let id: Int64?
  private let access: any CustomChannelsAccess

  public init(
    summary: CustomChannelSummary?, access: any CustomChannelsAccess
  ) {
    id = summary?.id
    self.access = access
    input = CustomChannelInput(
      groupId: summary?.groupId, name: summary?.name ?? "", logo: summary?.logo ?? "")
  }

  public var isEditing: Bool { id != nil }

  public func addHeader() {
    input.headers.append(CustomHeaderInput())
  }

  public func removeHeader(id: UUID) {
    input.headers.removeAll { $0.id == id }
  }

  public func save() async -> Bool {
    validationMessage = validate()
    guard validationMessage == nil else { return false }
    isSaving = true
    defer { isSaving = false }
    do {
      if let id {
        try await access.updateCustomChannel(id: id, input: input)
      } else {
        _ = try await access.createCustomChannel(input)
      }
      return true
    } catch {
      validationMessage = ActionableError((error as? ApiError) ?? .Internal).message
      return false
    }
  }

  private func validate() -> String? {
    if input.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return String(localized: "Enter a channel name.", bundle: .module)
    }
    if input.streamAddress.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return String(localized: "Enter the stream address.", bundle: .module)
    }
    if input.headers.contains(where: {
      $0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        != $0.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }) {
      return String(localized: "Complete both parts of each request detail.", bundle: .module)
    }
    return nil
  }
}
