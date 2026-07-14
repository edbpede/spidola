// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The set of engines this build can construct, and how to construct them.
///
/// Engines are **peers injected by the composition root**, never children of the playback feature
/// (doctrine §3.1) — so the app target owns this value and the playback slice receives it. That is
/// what keeps `FeaturePlayback` free of any dependency on `PlayerMPV`/`PlayerAV`: it holds engine
/// *identities* and asks the registry to build one.
///
/// Each factory builds a **fresh** engine. Engines are single-use by contract (`load` once, then
/// dispose), because the zap path destroys and rebuilds one per channel flip.
@MainActor
public struct EngineRegistry {
  private let factories: [EngineID: @MainActor () -> any PlaybackEngine]

  /// The engine to use when no override applies — MPVKit on tvOS (TECH_SPEC §8).
  public let platformDefault: EngineID

  public init(
    platformDefault: EngineID,
    factories: [EngineID: @MainActor () -> any PlaybackEngine]
  ) {
    self.platformDefault = platformDefault
    self.factories = factories
  }

  /// The engines actually available to the selection policy.
  public var registered: Set<EngineID> { Set(factories.keys) }

  /// Builds a fresh engine, or `nil` when `id` is not registered in this build.
  public func make(_ id: EngineID) -> (any PlaybackEngine)? {
    factories[id]?()
  }

  /// Resolves the policy (TECH_SPEC §8) against this registry and builds the winner.
  ///
  /// Returns `nil` only when the resolved engine cannot be built — which for a correctly composed
  /// app means the platform default is missing, a wiring bug the caller surfaces as one honest
  /// failure rather than a silent substitution.
  public func resolveAndMake(
    channelOverride: EngineID?,
    sourceOverride: EngineID?
  ) -> (any PlaybackEngine)? {
    let id = EngineSelection.resolve(
      channelOverride: channelOverride,
      sourceOverride: sourceOverride,
      platformDefault: platformDefault,
      registered: registered)
    return make(id)
  }
}
