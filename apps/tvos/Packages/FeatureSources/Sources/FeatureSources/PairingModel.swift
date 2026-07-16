// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// The pairing screen's phase. A closed set the view matches exhaustively.
public enum PairingState: Sendable {
  /// Working out this TV's LAN address, then bringing the server up.
  case starting
  /// The server is up: show the address, the code, and the QR.
  case waiting(PairingSession)
  /// A phone sent details. The screen hands them to the add-source form to be confirmed.
  case received(PairingSubmission)
  case failed(ActionableError)
}

/// Backs the LAN pairing screen (PRD §6.1): show an address and a code, wait for a phone, and hand
/// what it sends to the add-source flow for the person at the TV to confirm.
///
/// **The server's lifetime is the security model** (TECH_SPEC §12): it exists only while this
/// screen does. That is enforced structurally rather than by discipline — `run()` is driven from
/// the view's `.task`, so when the screen goes away the task is cancelled, the stream terminates,
/// and `SpidolaCore`'s `onTermination` stops the server. `stop()` here is the prompt, awaited
/// version of the same thing, not the only thing preventing a stray listener on the LAN.
///
/// A submission **pre-fills and never submits**. Anything on the LAN could have posted it, so the
/// person at the TV confirms — which is also why this model does not touch `addXtream` itself.
@MainActor
@Observable
public final class PairingModel {
  public private(set) var state: PairingState = .starting

  private let access: any PairingAccess
  private let resolveHost: () async -> String?

  public convenience init(access: any PairingAccess) {
    self.init(access: access, resolveHost: LanAddress.current)
  }

  /// The seam a test drives: address discovery reads this machine's real interfaces, which a test
  /// can neither predict nor stage.
  init(access: any PairingAccess, resolveHost: @escaping () async -> String?) {
    self.access = access
    self.resolveHost = resolveHost
  }

  /// Brings the server up and consumes its events until the screen goes away.
  ///
  /// Returns when the stream ends, which is either a failure or the caller's cancellation. It is
  /// meant to be driven by `.task`, whose cancellation is what takes the server down.
  public func run() async {
    guard let host = await resolveHost() else {
      // Deliberately *not* falling back to a `nil` host. That asks the core to infer the address
      // from the route out of this TV — the very inference `LanAddress` exists to replace — and on
      // the case that matters (a full-tunnel VPN) it would succeed at advertising an address no
      // phone can dial. A screen that says it cannot pair beats a QR code that goes nowhere.
      state = .failed(Self.noAddressError)
      return
    }
    for await event in access.pairing(host: host) {
      switch event {
      case .started(let session):
        state = .waiting(session)
      case .submission(let submission):
        state = .received(submission)
        // One submission is the whole job: the screen is done and the server should not outlive
        // it. Returning ends the `for await`, which terminates the stream and stops the server.
        return
      case .failed(let error):
        state = .failed(ActionableError(error))
        return
      }
    }
  }

  /// Stops the server now. The view calls this on the way out so the stop is prompt and awaited
  /// rather than trailing the screen.
  public func stop() async {
    await access.stopPairing()
  }

  /// Brings the server back up after a failure — the action behind `retry`.
  public func retry() async {
    state = .starting
    await run()
  }

  /// What to say when this TV has no address a phone could dial. Phrased as the two things that
  /// actually cause it, because "no network address" would send someone to check a cable that is
  /// already plugged in.
  private static let noAddressError = ActionableError(
    .InvalidInput(field: .address, issue: .unavailable))
}
