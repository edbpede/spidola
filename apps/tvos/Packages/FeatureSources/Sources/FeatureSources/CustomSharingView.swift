// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import Foundation
import SwiftUI
import core_api

public struct CustomSharingView: View {
  @State private var model: CustomSharingModel
  @State private var confirmReplace = false

  public init(access: any CustomChannelsAccess) {
    _model = State(initialValue: CustomSharingModel(access: access))
  }

  public var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xl) {
        exportSection
        importSection
        if let status = model.status {
          Text(status)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .navigationTitle(String(localized: "Share custom channels", bundle: .module))
    .confirmationDialog(
      String(localized: "Replace every custom channel?", bundle: .module),
      isPresented: $confirmReplace, titleVisibility: .visible
    ) {
      Button(String(localized: "Replace all", bundle: .module), role: .destructive) {
        Task { await model.importChannels(mode: .replace) }
      }
      Button(String(localized: "Cancel", bundle: .module), role: .cancel) {}
    } message: {
      Text(
        String(
          localized: "This removes the current custom lineup before importing.", bundle: .module))
    }
  }

  private var exportSection: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      Text(String(localized: "Export", bundle: .module))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      Text(
        String(
          localized:
            "The export can include private stream details. Share it only with people you trust.",
          bundle: .module)
      )
      .font(SpidolaType.caption)
      .foregroundStyle(SpidolaPalette.staticGray)
      Button(String(localized: "Prepare export", bundle: .module)) {
        Task { await model.prepareExport() }
      }
      if !model.exportContents.isEmpty {
        TextField(
          String(localized: "Export contents", bundle: .module), text: $model.exportContents,
          axis: .vertical
        )
        .font(SpidolaType.caption)
        .lineLimit(6...12)
        .padding(SpidolaSpacing.m)
        .background(SpidolaPalette.set)
        .accessibilityLabel(String(localized: "Export contents", bundle: .module))
      }
    }
  }

  private var importSection: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      Text(String(localized: "Import", bundle: .module))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      TextField(
        String(localized: "Paste shared channel data", bundle: .module),
        text: $model.importContents, axis: .vertical
      )
      .font(SpidolaType.caption)
      .lineLimit(6...12)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)
      .accessibilityLabel(String(localized: "Import contents", bundle: .module))
      HStack(spacing: SpidolaSpacing.m) {
        Button(String(localized: "Merge with current", bundle: .module)) {
          Task { await model.importChannels(mode: .merge) }
        }
        Button(String(localized: "Replace current", bundle: .module), role: .destructive) {
          confirmReplace = true
        }
      }
      .disabled(model.importContents.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }
  }
}
