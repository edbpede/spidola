// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rolling-window filter applied during parse (injected now, no clock).

/// The EPG retention window around an injected Unix timestamp.
///
/// A programme is retained when any part of its half-open interval overlaps the
/// configured window. The type owns durations rather than a clock so parsing is
/// deterministic in tests and callers can re-use the same parser at any instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpgWindow {
    behind_seconds: i64,
    ahead_seconds: i64,
}

impl EpgWindow {
    /// Builds a window from whole hours, saturating at the largest representable duration.
    #[must_use]
    pub fn from_hours(behind_hours: u32, ahead_hours: u32) -> Self {
        Self {
            behind_seconds: i64::from(behind_hours).saturating_mul(3_600),
            ahead_seconds: i64::from(ahead_hours).saturating_mul(3_600),
        }
    }

    /// The product default: six hours behind and three days ahead (PRD §6.6).
    #[must_use]
    pub fn default_epg() -> Self {
        Self::from_hours(6, 72)
    }

    /// Whether `[start_unix, end_unix)` overlaps the rolling window around `now_unix`.
    #[must_use]
    pub fn includes(self, now_unix: i64, start_unix: i64, end_unix: i64) -> bool {
        let lower = now_unix.saturating_sub(self.behind_seconds);
        let upper = now_unix.saturating_add(self.ahead_seconds);
        end_unix > lower && start_unix < upper
    }
}

impl Default for EpgWindow {
    fn default() -> Self {
        Self::default_epg()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_uses_half_open_intervals() {
        let window = EpgWindow::from_hours(1, 2);
        let now = 10_000;

        assert!(window.includes(now, 9_000, 9_001));
        assert!(window.includes(now, 11_999, 12_001));
        assert!(!window.includes(now, 0, 6_400));
        assert!(!window.includes(now, 17_200, 18_000));
    }
}
