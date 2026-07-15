// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Darwin
import Network

/// Finds the TV's own LAN IPv4 address — the one a phone on the same network can dial, and the one
/// the pairing screen advertises.
///
/// **Why the shell answers this and not the core.** `core-pair` infers the address by asking the
/// kernel which interface leaves the host. That is right on a plain LAN and wrong behind a
/// full-tunnel VPN, where the route out *is* the tunnel and the LAN address sits on an interface
/// the probe never sees. Its docs carry the measurement: Wi-Fi at `192.168.50.98`, tunnel at
/// `100.80.166.175`, and every probe destination — public, private, broadcast, multicast —
/// returned the tunnel. No choice of destination fixes it, because the only one that would work is
/// inside the subnet being discovered. Enumerating interfaces answers it properly, and the platform
/// is what can enumerate them.
///
/// So this asks two questions the core cannot, and combines them:
///
/// - **Which interfaces are the LAN?** `Network` answers by *type*, so nothing here rests on `en0`
///   being called `en0` — a name-prefix heuristic would be guessing at exactly the point this code
///   exists to stop guessing.
/// - **What address does each carry?** `getifaddrs`, which is the only way to ask on Darwin.
///
/// The tunnel is then excluded twice over: it is neither Wi-Fi nor Ethernet, and its CGNAT address
/// is not RFC1918.
enum LanAddress {
  /// The address to advertise, or `nil` when this TV has no dialable LAN address.
  ///
  /// `nil` is a real answer, not a failure to try, and the caller must **not** turn it into a `nil`
  /// `host` for the core: that would ask the core to fall back to the very inference this exists to
  /// replace, and on the VPN case it would succeed at advertising an address no phone can reach.
  /// A URL that cannot be dialed is worse than a screen that says so.
  static func current() async -> String? {
    let names = await lanInterfaceNames()
    guard !names.isEmpty else { return nil }
    return ipv4(on: names)
  }

  /// The names of the interfaces `Network` considers Wi-Fi or wired Ethernet.
  ///
  /// Bounded: `NWPathMonitor` reports the current path as soon as it starts, but a monitor that
  /// never fires would hang the pairing screen on a spinner forever, and "we could not tell" is a
  /// thing this screen can say.
  private static func lanInterfaceNames() async -> Set<String> {
    await withTaskGroup(of: Set<String>?.self) { group in
      group.addTask { await firstPathInterfaces() }
      group.addTask {
        try? await Task.sleep(for: .seconds(2))
        return nil
      }
      let first = await group.next() ?? nil
      group.cancelAll()
      return first ?? []
    }
  }

  private static func firstPathInterfaces() async -> Set<String> {
    let monitor = NWPathMonitor()
    let paths = AsyncStream<NWPath> { continuation in
      monitor.pathUpdateHandler = { continuation.yield($0) }
      // Taking the first value ends the stream, which cancels the monitor — no second resume to
      // guard against, which a raw continuation here would need.
      continuation.onTermination = { _ in monitor.cancel() }
      monitor.start(queue: .global(qos: .userInitiated))
    }
    for await path in paths {
      return Set(
        path.availableInterfaces
          .filter { $0.type == .wifi || $0.type == .wiredEthernet }
          .map(\.name))
    }
    return []
  }

  /// The first running, non-loopback IPv4 address on one of `names` that a phone could dial.
  private static func ipv4(on names: Set<String>) -> String? {
    var head: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&head) == 0, let head else { return nil }
    defer { freeifaddrs(head) }

    for pointer in sequence(first: head, next: { $0.pointee.ifa_next }) {
      let interface = pointer.pointee
      guard let address = interface.ifa_addr, address.pointee.sa_family == UInt8(AF_INET) else {
        continue
      }
      let flags = Int32(interface.ifa_flags)
      guard flags & IFF_UP != 0, flags & IFF_RUNNING != 0, flags & IFF_LOOPBACK == 0 else {
        continue
      }
      guard names.contains(String(cString: interface.ifa_name)) else { continue }
      guard let text = presentation(of: address), isPrivateLanAddress(text) else { continue }
      return text
    }
    return nil
  }

  /// The numeric form of a `sockaddr`. `getnameinfo` with `NI_NUMERICHOST` rather than `inet_ntop`
  /// so no pointer is rebound to a narrower type.
  private static func presentation(of address: UnsafeMutablePointer<sockaddr>) -> String? {
    var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
    let result = getnameinfo(
      address, socklen_t(address.pointee.sa_len),
      &buffer, socklen_t(buffer.count),
      nil, 0, NI_NUMERICHOST)
    guard result == 0 else { return nil }
    return String(cString: buffer)
  }

  /// Whether an IPv4 address is RFC1918 — the ranges `core-pair` accepts *and* a phone on the same
  /// network can actually reach.
  ///
  /// Link-local and loopback are deliberately absent even though the core would take them: a phone
  /// cannot dial `127.0.0.1`, and a `169.254.x.x` address means this TV never got a lease, so
  /// advertising either would produce a URL that resolves to nothing. Narrower than the core's
  /// predicate on purpose — the core is judging "is this dialable in principle", this is choosing
  /// what to put on screen.
  static func isPrivateLanAddress(_ text: String) -> Bool {
    let octets = text.split(separator: ".").compactMap { UInt8($0) }
    guard octets.count == 4 else { return false }
    switch (octets[0], octets[1]) {
    case (10, _): return true
    case (172, 16...31): return true
    case (192, 168): return true
    default: return false
    }
  }
}
