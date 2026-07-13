// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import OSLog
import core_api

/// Drains the core's `tracing` pipeline into OSLog (TECH_SPEC §4.8): the app bundle id is the
/// subsystem and each core span target is the category, so Console filters show the whole
/// pipeline; levels map one-to-one. The core renders and sanitizes messages (its secret types
/// redact Debug), so no credential-shaped value reaches here. UniFFI may call `log` on any thread.
public final class OSLogSink: LogSink {
  private let subsystem: String

  public init(subsystem: String = "dev.spidola.tv") {
    self.subsystem = subsystem
  }

  public func log(record: LogRecord) {
    let logger = Logger(subsystem: subsystem, category: record.target)
    let message = record.message
    switch record.level {
    case .error: logger.error("\(message, privacy: .public)")
    case .warn: logger.warning("\(message, privacy: .public)")
    case .info: logger.info("\(message, privacy: .public)")
    case .debug: logger.debug("\(message, privacy: .public)")
    case .trace: logger.trace("\(message, privacy: .public)")
    @unknown default: logger.info("\(message, privacy: .public)")
    }
  }
}
