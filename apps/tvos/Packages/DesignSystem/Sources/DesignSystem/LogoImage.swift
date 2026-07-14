// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI
import UIKit

/// A lazy, disk-cached channel logo (TECH_SPEC §6: "a small async image pipeline over URLSession
/// with a disk cache capped per the settings value"). Artwork is the one subsystem allowed network
/// access outside the core, because logo URLs are public by nature and never touch credentials.
/// While loading or on failure it shows a neutral placeholder, so a broken logo never blocks the
/// grid. Decode happens off the main actor; only the finished image crosses back.
public struct LogoImage: View {
  private let url: URL?
  private let cornerRadius: CGFloat

  @State private var image: UIImage?
  @State private var didFail = false

  public init(url: String?, cornerRadius: CGFloat = 8) {
    self.url = url.flatMap(URL.init(string:))
    self.cornerRadius = cornerRadius
  }

  public var body: some View {
    ZStack {
      if let image {
        Image(uiImage: image)
          .resizable()
          .aspectRatio(contentMode: .fit)
      } else {
        placeholder
      }
    }
    .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    .task(id: url) { await load() }
    .accessibilityHidden(true)
  }

  private var placeholder: some View {
    ZStack {
      SpidolaPalette.set
      Image(systemName: didFail ? "photo" : "tv")
        .font(.system(size: 34))
        .foregroundStyle(SpidolaPalette.staticGray)
    }
  }

  private func load() async {
    image = nil
    didFail = false
    guard let url else { return }
    if let loaded = await LogoCache.shared.image(for: url) {
      image = loaded
    } else {
      didFail = true
    }
  }
}

/// The shared logo pipeline: a `URLSession` whose `URLCache` is capped on disk, plus a small
/// in-memory image cache. An `actor` so the caches are touched from one isolation domain without
/// locks. The disk cap mirrors the PRD's "capped disk cache" requirement; the settings-driven
/// value wires in when the settings surface lands (Phase 6).
actor LogoCache {
  static let shared = LogoCache()

  private let session: URLSession
  private let memory = NSCache<NSURL, UIImage>()

  private init() {
    let cache = URLCache(
      memoryCapacity: 8 * 1024 * 1024,
      diskCapacity: 96 * 1024 * 1024,
      diskPath: "spidola-logos")
    let config = URLSessionConfiguration.default
    config.urlCache = cache
    config.requestCachePolicy = .returnCacheDataElseLoad
    config.timeoutIntervalForRequest = 15
    session = URLSession(configuration: config)
    memory.countLimit = 256
  }

  func image(for url: URL) async -> UIImage? {
    if let cached = memory.object(forKey: url as NSURL) { return cached }
    guard let (data, response) = try? await session.data(from: url),
      (response as? HTTPURLResponse).map({ (200..<300).contains($0.statusCode) }) ?? true,
      let image = UIImage(data: data)
    else {
      return nil
    }
    memory.setObject(image, forKey: url as NSURL)
    return image
  }
}
