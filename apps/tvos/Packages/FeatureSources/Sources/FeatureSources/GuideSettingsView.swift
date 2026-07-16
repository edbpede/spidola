// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

public struct GuideSettingsView: View {
  @State private var model: GuideSettingsModel
  private let sourceName: String
  private let acceptsUrl: Bool
  @FocusState private var focused: Focus?

  public init(sourceId: Int64, sourceName: String, acceptsUrl: Bool, access: any EpgAccess) {
    _model = State(initialValue: GuideSettingsModel(sourceId: sourceId, access: access))
    self.sourceName = sourceName
    self.acceptsUrl = acceptsUrl
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Programme guide", bundle: .module))
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Checking guide…", bundle: .module))
    case .empty:
      editor(hasFeed: false)
    case .failed(let error):
      actionableError(
        error, retry: { Task { await model.load() } },
        goBack: { Task { await model.load() } },
        fixInput: { model.feedUrl = "" })
    case .ready(let snapshot):
      editor(hasFeed: snapshot.hasFeed)
    }
  }

  private func editor(hasFeed: Bool) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.l) {
        Text(sourceName)
          .font(SpidolaType.display)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
        Text(
          acceptsUrl
            ? String(
              localized: "Add the guide address supplied with this playlist.", bundle: .module)
            : String(
              localized: "This account supplies its programme guide directly.", bundle: .module)
        )
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.staticGray)

        if acceptsUrl {
          Text(String(localized: "Guide address", bundle: .module))
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
          TextField(String(localized: "https://…", bundle: .module), text: $model.feedUrl)
            .textContentType(.URL)
            .textInputAutocapitalization(.never)
            .font(SpidolaType.body)
            .padding(SpidolaSpacing.m)
            .background(SpidolaPalette.set)
            .focused($focused, equals: .url)
            .accessibilityLabel(String(localized: "Guide address", bundle: .module))
          HStack(spacing: SpidolaSpacing.m) {
            actionButton(
              String(localized: "Save guide", bundle: .module), focus: .save,
              primary: true
            ) { Task { await model.saveFeed() } }
            if hasFeed {
              actionButton(String(localized: "Remove guide", bundle: .module), focus: .remove) {
                Task { await model.clearFeed() }
              }
            }
          }
        }

        actionButton(String(localized: "Refresh guide", bundle: .module), focus: .refresh) {
          Task { await model.refresh() }
        }
        .disabled(!hasFeed)

        Text(status(hasFeed: hasFeed))
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .onAppear { focused = acceptsUrl ? .url : .refresh }
  }

  private func status(hasFeed: Bool) -> String {
    switch model.refreshStatus {
    case .idle:
      return hasFeed
        ? String(localized: "Guide ready to refresh.", bundle: .module)
        : String(localized: "Add a guide address before refreshing.", bundle: .module)
    case .running(let seen):
      return String(
        localized: "Reading guide: \(seen) programmes", bundle: .module,
        comment: "Guide refresh progress. The number is a programme count.")
    case .complete(let inserted):
      return String(
        localized: "Guide updated: \(inserted) programmes", bundle: .module,
        comment: "Guide refresh result. The number is a programme count.")
    }
  }

  private func actionButton(
    _ title: String, focus: Focus, primary: Bool = false, action: @escaping () -> Void
  ) -> some View {
    Button(title, action: action)
      .buttonStyle(.plain)
      .font(SpidolaType.body)
      .padding(.horizontal, SpidolaSpacing.l)
      .padding(.vertical, SpidolaSpacing.m)
      .background(primary ? SpidolaPalette.testCardAmber : SpidolaPalette.set)
      .foregroundStyle(primary ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite)
      .focused($focused, equals: focus)
      .spidolaFocusRing(isFocused: focused == focus)
  }

  private enum Focus: Hashable { case url, save, remove, refresh }
}
