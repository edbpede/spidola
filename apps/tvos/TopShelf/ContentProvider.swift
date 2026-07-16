// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@preconcurrency import TVServices

final class ContentProvider: TVTopShelfContentProvider {
  override func loadTopShelfContent() async -> (any TVTopShelfContent)? {
    guard let snapshot = try? TopShelfSnapshotStore.read() else { return nil }
    let items = snapshot.favorites.compactMap(makeItem)
    guard items.isEmpty == false else { return nil }

    let collection = TVTopShelfItemCollection(items: items)
    collection.title = String(localized: "Favorites")
    return TVTopShelfSectionedContent(sections: [collection])
  }

  private func makeItem(_ snapshot: TopShelfSnapshot.Item) -> TVTopShelfSectionedItem? {
    guard let deepLink = snapshot.deepLink else { return nil }
    let item = TVTopShelfSectionedItem(identifier: snapshot.identifier)
    item.title = snapshot.title
    item.imageShape = .hdtv
    if let imageURL = snapshot.imageURL {
      item.setImageURL(imageURL, for: [.screenScale1x, .screenScale2x])
    }
    let action = TVTopShelfAction(url: deepLink)
    item.displayAction = action
    item.playAction = action
    return item
  }
}
