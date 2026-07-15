// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The typed settings vocabulary and its defaults (PRD §6.9, TECH_SPEC §4.6).
//!
//! `core-db`'s settings table is a flat key→value store of opaque strings; this module is the
//! **vocabulary** that store speaks — the key names, the closed value sets, and the code
//! defaults — while [`SettingsService`](crate::services::SettingsService) is the service that
//! reads and writes it. The two are separate because they change for different reasons: a new
//! setting extends the vocabulary, while the service's read/write discipline does not move.
//!
//! Two rules shape the design. First, **every setting has a code default and the app is fully
//! usable without ever opening settings** (PRD §6.9) — so a value is only ever *stored* when the
//! user changes it, an absent key reads as its default, and an unrecognized stored value (a
//! downgraded app meeting a newer enum) falls back to the default rather than failing. That is
//! what [`AppSettings::default`] and the `from_stored` conversions below encode, and what
//! `defaults_need_no_stored_values` proves. Second, **nothing here is stringly-typed across the
//! FFI**: closed sets are enums the shells match exhaustively, and the key strings never cross
//! the boundary.
//!
//! The engine settings are the deliberate exception to "closed sets are enums": an engine is an
//! **opaque key**, not a core-defined enum, exactly as `preferred_engine` already is on
//! [`ChannelOverrides`](crate::records::ChannelOverrides). The core has no opinion on which
//! engines exist — the shells' selection policy resolves that (TECH_SPEC §8) — so the core
//! stores the key and stays out of it. `None` means "no override; use the platform default".
//!
//! No secret ever lands here (TECH_SPEC §12).

use crate::logging::LogLevel;

/// The settings-table keys this vocabulary owns. Namespaced by area, matching the convention
/// `recents.enabled` established in [`RecentsService`](crate::services::RecentsService).
///
/// Crate-private on purpose: keys are an implementation detail of the store, and a shell that
/// could name one could invent an untyped setting outside this vocabulary.
pub(crate) mod keys {
    /// Opaque global default-engine key; absent means "platform default".
    pub(crate) const DEFAULT_ENGINE: &str = "playback.default_engine";
    /// [`BufferingProfile`](super::BufferingProfile).
    pub(crate) const BUFFERING: &str = "playback.buffering";
    /// [`SubtitleSize`](super::SubtitleSize).
    pub(crate) const SUBTITLE_SIZE: &str = "subtitles.size";
    /// [`SubtitleBackground`](super::SubtitleBackground).
    pub(crate) const SUBTITLE_BACKGROUND: &str = "subtitles.background";
    /// BCP-47 language tag; absent means "follow the system language".
    pub(crate) const LANGUAGE: &str = "ui.language";
    /// [`InterfaceDensity`](super::InterfaceDensity).
    pub(crate) const DENSITY: &str = "ui.density";
    /// Recently-watched off-switch. Owned by `RecentsService`, read here for the snapshot.
    pub(crate) const RECENTS_ENABLED: &str = "recents.enabled";
    /// Days of recently-watched history to keep.
    pub(crate) const RECENTS_RETENTION_DAYS: &str = "recents.retention_days";
    /// Hours of EPG to keep ahead of now.
    pub(crate) const EPG_WINDOW_AHEAD_HOURS: &str = "epg.window_ahead_hours";
    /// Hours of EPG to keep behind now.
    pub(crate) const EPG_WINDOW_BEHIND_HOURS: &str = "epg.window_behind_hours";
    /// Image disk-cache ceiling in megabytes (the shells' artwork pipelines read it).
    pub(crate) const IMAGE_CACHE_MAX_MB: &str = "cache.image_max_mb";
    /// [`LogLevel`](crate::logging::LogLevel) for the diagnostics screen.
    pub(crate) const LOG_LEVEL: &str = "diagnostics.log_level";

    /// Per-source opaque engine-override key — the middle tier of the PRD §6.3 selection
    /// policy. Namespaced under `playback.` like [`DEFAULT_ENGINE`] and [`BUFFERING`], because
    /// all three are one area's settings and only differ in scope.
    pub(crate) fn source_engine(source_id: i64) -> String {
        format!("playback.engine.source.{source_id}")
    }

    /// Per-channel opaque engine-override key — the **top** tier of the selection policy, and
    /// what the playback slice's "remember for this channel" writes.
    ///
    /// Keyed by the channel's stable identity hash rather than its rowid, and living here in
    /// the settings table rather than on `channels.preferred_engine`, for the same reason
    /// favorites do: a refresh replaces every channel row wholesale (staging-and-swap,
    /// TECH_SPEC §4.4), so an override stored on the row would be destroyed the next time the
    /// source refreshed. Identity survives refresh; a rowid does not.
    pub(crate) fn channel_engine(source_id: i64, identity: i64) -> String {
        format!("playback.engine.channel.{source_id}.{identity}")
    }
}

/// How much latency the viewer trades for resilience, mapped to engine parameters by each shell
/// (PRD §6.9).
///
/// The variants mirror `PlayerContract.BufferingProfile` / `player-contract`'s
/// `BufferingProfile` exactly, including their stored spellings. That vocabulary was settled in
/// Phase 5 and is the engine-neutral one both shells already speak — its own docs say "the
/// settings screen speaks this vocabulary" — so the core adopts it rather than inventing a
/// second, lossy one. PRD §6.9's "low-latency vs. stable" is a summary of the axis, not a
/// statement that it has two positions; `Balanced` is the middle the shells default to, and a
/// two-variant core enum would have had no honest value to map it onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum BufferingProfile {
    /// Smallest buffer: fastest zap, least tolerant of a jittery source.
    Low,
    /// The default trade-off.
    #[default]
    Balanced,
    /// Largest buffer: slowest to start, rides out a bad connection.
    Generous,
}

/// Subtitle glyph size, resolved to platform points by each shell (PRD §6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum SubtitleSize {
    /// Below the default.
    Small,
    /// The default.
    #[default]
    Medium,
    /// Above the default.
    Large,
}

/// What sits behind subtitle text, so it stays legible over bright video (PRD §6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum SubtitleBackground {
    /// Glyphs only.
    None,
    /// A soft drop shadow — legible on most material without a visible box.
    #[default]
    Shadow,
    /// An opaque plate behind the text.
    Solid,
}

/// How much breathing room lists and rows get (PRD §6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum InterfaceDensity {
    /// Fewer, larger rows — the 10-foot default.
    #[default]
    Comfortable,
    /// More rows per screen, for users who prefer density to reach.
    Compact,
}

/// Every setting resolved to a value: stored where the user set one, code default otherwise.
///
/// Flat and owned, per the boundary rules (TECH_SPEC §5). The shells render this directly;
/// there is no "unset" state to represent because [`Default`] has already resolved it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppSettings {
    /// Opaque global default-engine key; `None` means the platform default (TECH_SPEC §8).
    pub default_engine: Option<String>,
    /// Buffering profile.
    pub buffering: BufferingProfile,
    /// Subtitle glyph size.
    pub subtitle_size: SubtitleSize,
    /// Subtitle backing treatment.
    pub subtitle_background: SubtitleBackground,
    /// BCP-47 UI language tag; `None` means follow the system language.
    pub language: Option<String>,
    /// List/row density.
    pub density: InterfaceDensity,
    /// Whether recently-watched recording is on (PRD §6.5 off-switch).
    pub recents_enabled: bool,
    /// Days of recently-watched history to keep.
    pub recents_retention_days: u32,
    /// Hours of EPG kept ahead of now (PRD §6.6; consumed when EPG ingest lands).
    pub epg_window_ahead_hours: u32,
    /// Hours of EPG kept behind now (PRD §6.6; consumed when EPG ingest lands).
    pub epg_window_behind_hours: u32,
    /// Image disk-cache ceiling in megabytes, read by each shell's artwork pipeline.
    pub image_cache_max_mb: u32,
    /// Diagnostics log level (PRD §6.9, TECH_SPEC §4.8).
    pub log_level: LogLevel,
}

impl Default for AppSettings {
    /// The "never opened settings" configuration (PRD §6.9). Each value is the one a household
    /// member should never need to change; the EPG window matches PRD §6.6's stated defaults
    /// (3 days ahead, 6 hours behind).
    fn default() -> Self {
        Self {
            default_engine: None,
            buffering: BufferingProfile::default(),
            subtitle_size: SubtitleSize::default(),
            subtitle_background: SubtitleBackground::default(),
            language: None,
            density: InterfaceDensity::default(),
            recents_enabled: true,
            recents_retention_days: 90,
            epg_window_ahead_hours: 72,
            epg_window_behind_hours: 6,
            image_cache_max_mb: 256,
            log_level: LogLevel::default(),
        }
    }
}

/// A closed setting value that round-trips through the flat store.
///
/// Implementors decide their own stored spelling and, crucially, their own tolerance: an
/// unrecognized value reads back as the default rather than failing, so a shell downgraded past
/// a newly-added variant degrades to the default instead of refusing to load settings.
pub(crate) trait StoredValue: Sized + Default {
    /// The stable string this value persists as. Changing one is a migration, not an edit.
    fn as_stored(&self) -> &'static str;

    /// Parses a stored spelling, or `None` if unrecognized.
    fn parse_stored(raw: &str) -> Option<Self>;

    /// Resolves a possibly-absent, possibly-unrecognized stored value to a usable one.
    fn from_stored(raw: Option<&str>) -> Self {
        raw.and_then(Self::parse_stored).unwrap_or_default()
    }
}

impl StoredValue for BufferingProfile {
    /// Deliberately the shells' `BufferingProfile` raw values: the same setting must round-trip
    /// whether it was written by the settings screen through this vocabulary or by the playback
    /// slice through its own contract type.
    fn as_stored(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Balanced => "balanced",
            Self::Generous => "generous",
        }
    }

    fn parse_stored(raw: &str) -> Option<Self> {
        match raw {
            "low" => Some(Self::Low),
            "balanced" => Some(Self::Balanced),
            "generous" => Some(Self::Generous),
            _ => None,
        }
    }
}

impl StoredValue for SubtitleSize {
    fn as_stored(&self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    fn parse_stored(raw: &str) -> Option<Self> {
        match raw {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }
}

impl StoredValue for SubtitleBackground {
    fn as_stored(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Shadow => "shadow",
            Self::Solid => "solid",
        }
    }

    fn parse_stored(raw: &str) -> Option<Self> {
        match raw {
            "none" => Some(Self::None),
            "shadow" => Some(Self::Shadow),
            "solid" => Some(Self::Solid),
            _ => None,
        }
    }
}

impl StoredValue for InterfaceDensity {
    fn as_stored(&self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }

    fn parse_stored(raw: &str) -> Option<Self> {
        match raw {
            "comfortable" => Some(Self::Comfortable),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }
}

impl StoredValue for LogLevel {
    fn as_stored(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    fn parse_stored(raw: &str) -> Option<Self> {
        match raw {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }
}

/// Resolves a stored count, falling back to `default` when absent or unparseable — the same
/// tolerance the [`StoredValue`] enums get, for the numeric settings.
pub(crate) fn count_from(raw: Option<&str>, default: u32) -> u32 {
    raw.and_then(|value| value.parse().ok()).unwrap_or(default)
}

/// Interprets the recently-watched off-switch: on unless explicitly `"0"` (PRD §6.5).
///
/// Kept bit-identical to `RecentsService`'s own reading of the same key — the snapshot must not
/// disagree with the service that enforces it.
pub(crate) fn recents_enabled_from(raw: Option<&str>) -> bool {
    raw != Some("0")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Every closed set survives a write→read round trip at every variant, so a stored setting
    /// still means what it meant when it was written.
    #[test]
    fn stored_values_round_trip() {
        fn round_trip<T: StoredValue + PartialEq + std::fmt::Debug>(values: &[T]) {
            for value in values {
                assert_eq!(
                    T::parse_stored(value.as_stored()).as_ref(),
                    Some(value),
                    "{:?} did not survive a round trip",
                    value.as_stored()
                );
            }
        }
        round_trip(&[
            BufferingProfile::Low,
            BufferingProfile::Balanced,
            BufferingProfile::Generous,
        ]);
        round_trip(&[
            SubtitleSize::Small,
            SubtitleSize::Medium,
            SubtitleSize::Large,
        ]);
        round_trip(&[
            SubtitleBackground::None,
            SubtitleBackground::Shadow,
            SubtitleBackground::Solid,
        ]);
        round_trip(&[InterfaceDensity::Comfortable, InterfaceDensity::Compact]);
        round_trip(&[
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ]);
    }

    /// An absent key and an unrecognized value both resolve to the default — a fresh install and
    /// a downgraded shell each get a usable setting instead of an error (PRD §6.9).
    #[test]
    fn absent_and_unrecognized_values_fall_back_to_the_default() {
        assert_eq!(
            BufferingProfile::from_stored(None),
            BufferingProfile::Balanced
        );
        assert_eq!(
            BufferingProfile::from_stored(Some("from-a-newer-app")),
            BufferingProfile::Balanced
        );
        assert_eq!(SubtitleSize::from_stored(Some("")), SubtitleSize::Medium);
        assert_eq!(LogLevel::from_stored(Some("verbose")), LogLevel::Info);
        assert_eq!(count_from(None, 72), 72);
        assert_eq!(count_from(Some("not a number"), 72), 72);
        assert_eq!(count_from(Some("12"), 72), 12);
    }

    /// The off-switch reading must match `RecentsService`'s, which is authoritative.
    #[test]
    fn recents_off_switch_matches_the_service_reading() {
        assert!(recents_enabled_from(None));
        assert!(recents_enabled_from(Some("1")));
        assert!(!recents_enabled_from(Some("0")));
    }

    /// "The app must be fully usable without ever opening settings" (PRD §6.9): resolving every
    /// setting against an *empty* store must produce exactly [`AppSettings::default`].
    #[test]
    fn defaults_need_no_stored_values() {
        let resolved = AppSettings {
            default_engine: None,
            buffering: BufferingProfile::from_stored(None),
            subtitle_size: SubtitleSize::from_stored(None),
            subtitle_background: SubtitleBackground::from_stored(None),
            language: None,
            density: InterfaceDensity::from_stored(None),
            recents_enabled: recents_enabled_from(None),
            recents_retention_days: count_from(None, 90),
            epg_window_ahead_hours: count_from(None, 72),
            epg_window_behind_hours: count_from(None, 6),
            image_cache_max_mb: count_from(None, 256),
            log_level: LogLevel::from_stored(None),
        };
        assert_eq!(resolved, AppSettings::default());
    }

    /// The buffering spellings are a **contract with the shells**, not this module's private
    /// business: `PlayerContract.BufferingProfile` (Swift) and `player-contract`'s
    /// `BufferingProfile` (Kotlin) use these exact raw values, and the playback slice writes the
    /// same setting through its own contract type. Changing one side alone silently splits the
    /// setting in two — a user's choice made on the settings screen would stop reaching the
    /// player. Pinned here so that edit fails loudly instead.
    #[test]
    fn buffering_spellings_match_the_shells_vocabulary() {
        assert_eq!(BufferingProfile::Low.as_stored(), "low");
        assert_eq!(BufferingProfile::Balanced.as_stored(), "balanced");
        assert_eq!(BufferingProfile::Generous.as_stored(), "generous");
        // And the default must be the same middle the shells default to.
        assert_eq!(BufferingProfile::default(), BufferingProfile::Balanced);
    }

    /// The EPG window defaults are PRD §6.6's stated numbers, not arbitrary ones.
    #[test]
    fn epg_window_defaults_match_the_prd() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.epg_window_ahead_hours, 72); // 3 days ahead
        assert_eq!(defaults.epg_window_behind_hours, 6); // 6 hours behind
    }

    /// Per-source engine keys are namespaced by id, so two sources never collide.
    #[test]
    fn per_source_engine_keys_are_distinct() {
        assert_eq!(keys::source_engine(1), "playback.engine.source.1");
        assert_ne!(keys::source_engine(1), keys::source_engine(2));
    }

    /// The three engine tiers (PRD §6.3: channel → source → platform default) must occupy three
    /// distinct keys, or setting one would silently clobber another.
    #[test]
    fn the_three_engine_tiers_never_collide() {
        let channel = keys::channel_engine(1, 42);
        let source = keys::source_engine(1);
        assert_eq!(channel, "playback.engine.channel.1.42");
        assert_ne!(channel, source);
        assert_ne!(channel, keys::DEFAULT_ENGINE);
        assert_ne!(source, keys::DEFAULT_ENGINE);
        // Two channels of one source, and one identity across two sources, all stay distinct.
        assert_ne!(keys::channel_engine(1, 42), keys::channel_engine(1, 43));
        assert_ne!(keys::channel_engine(1, 42), keys::channel_engine(2, 42));
    }

    /// A negative identity (the hash is a `u64` stored as `i64`, so half of them are negative)
    /// must still produce a usable, unambiguous key.
    #[test]
    fn negative_channel_identities_key_cleanly() {
        assert_eq!(
            keys::channel_engine(3, -8_000_000_000),
            "playback.engine.channel.3.-8000000000"
        );
        assert_ne!(keys::channel_engine(3, -1), keys::channel_engine(3, 1));
    }
}
