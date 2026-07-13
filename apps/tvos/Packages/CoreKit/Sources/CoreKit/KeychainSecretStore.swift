// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import Security
import core_api

/// The host-secrets callback (TECH_SPEC §12): the core stores only opaque keys and calls back
/// here to read or write the actual secret, which lives in the Keychain — never in SQLite or the
/// log stream. UniFFI may call these methods from any core thread; the Keychain API is thread-safe.
public final class KeychainSecretStore: SecretStore {
  private let service: String

  public init(service: String = "dev.spidola.tv.secrets") {
    self.service = service
  }

  public func get(key: String) throws -> String? {
    var query = baseQuery(for: key)
    query[kSecReturnData as String] = true
    query[kSecMatchLimit as String] = kSecMatchLimitOne
    var item: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &item)
    if status == errSecItemNotFound { return nil }
    guard status == errSecSuccess,
      let data = item as? Data,
      let value = String(data: data, encoding: .utf8)
    else {
      throw KeychainError.unexpected(status)
    }
    return value
  }

  public func set(key: String, value: String) throws {
    let data = Data(value.utf8)
    let query = baseQuery(for: key)
    let update = SecItemUpdate(
      query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
    if update == errSecSuccess { return }
    if update == errSecItemNotFound {
      var insert = query
      insert[kSecValueData as String] = data
      let added = SecItemAdd(insert as CFDictionary, nil)
      guard added == errSecSuccess else { throw KeychainError.unexpected(added) }
      return
    }
    throw KeychainError.unexpected(update)
  }

  public func delete(key: String) throws {
    let status = SecItemDelete(baseQuery(for: key) as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
      throw KeychainError.unexpected(status)
    }
  }

  private func baseQuery(for key: String) -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: key,
    ]
  }
}

/// A failure from the Keychain host callback, carrying the raw `OSStatus` for diagnostics.
public enum KeychainError: Error {
  case unexpected(OSStatus)
}
