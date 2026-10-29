//! User preferences, persisted as a TOML file next to the SQLite database
//! rather than as columns on it (see docs/REFACTORING.md item 4). A
//! preference is read once at startup and written one at a time when the
//! user changes it — not relational data, so it doesn't need a database.
//!
//! Field shapes intentionally match what the old `user_profile` columns
//! held (plain strings for the enum-backed ones), so the GUI's existing
//! `from_db_str`/`as_db_str` round-trip needs no changes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui_theme: String,
    pub board_theme: String,
    pub piece_set: String,
    pub analysis_quality: String,
    pub sfx_volume: f64,
    pub custom_use_time_limit: bool,
    pub custom_think_time_ms: i64,
    pub custom_max_depth: i64,
}

impl Default for Settings {
    /// Exactly the defaults the old DB columns used, so migrating an
    /// existing install and starting a fresh install land on the same
    /// values either way.
    fn default() -> Self {
        Settings {
            ui_theme: "light".to_string(),
            board_theme: "walnut".to_string(),
            piece_set: "cburnett".to_string(),
            analysis_quality: "standard".to_string(),
            sfx_volume: crate::sound::DEFAULT_VOLUME as f64,
            custom_use_time_limit: true,
            custom_think_time_ms: 5000,
            custom_max_depth: 12,
        }
    }
}

fn settings_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("focalors").join("settings.toml")
    } else {
        PathBuf::from("settings.toml")
    }
}

/// Load settings from disk, falling back to defaults on a missing file,
/// an unreadable file, or a parse error — a corrupt or absent settings
/// file should never stop the app from starting.
pub fn load() -> Settings {
    load_from(&settings_path())
}

/// Write the full settings file. Best-effort: the caller treats a failure
/// the same way the old DB writes did — log nothing, just keep running
/// with the in-memory value, and try again on the next change.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    save_to(&settings_path(), settings)
}

// Path-parameterized so tests can round-trip through a temp file instead
// of the real settings_path() — exercising the real file I/O without any
// risk of clobbering the developer's own settings.toml when the test
// suite runs.

fn load_from(path: &std::path::Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

fn save_to(path: &std::path::Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches what the old DB columns defaulted to, so an existing
    /// install's migration and a brand-new install land on the same
    /// values either way.
    #[test]
    fn defaults_match_the_old_db_column_defaults() {
        let s = Settings::default();
        assert_eq!(s.ui_theme, "light");
        assert_eq!(s.board_theme, "walnut");
        assert_eq!(s.piece_set, "cburnett");
        assert_eq!(s.analysis_quality, "standard");
        assert_eq!(s.sfx_volume, crate::sound::DEFAULT_VOLUME as f64);
        assert!(s.custom_use_time_limit);
        assert_eq!(s.custom_think_time_ms, 5000);
        assert_eq!(s.custom_max_depth, 12);
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let path = std::env::temp_dir().join("focalors_settings_test_missing.toml");
        let _ = std::fs::remove_file(&path);
        let loaded = load_from(&path);
        assert_eq!(loaded.ui_theme, Settings::default().ui_theme);
    }

    #[test]
    fn save_then_load_round_trips_every_field() {
        let path = std::env::temp_dir().join("focalors_settings_test_roundtrip.toml");
        let settings = Settings {
            ui_theme: "dark".to_string(),
            board_theme: "glacier".to_string(),
            piece_set: "rhosgfx".to_string(),
            analysis_quality: "deep".to_string(),
            sfx_volume: 0.73,
            custom_use_time_limit: false,
            custom_think_time_ms: 2500,
            custom_max_depth: 9,
        };
        save_to(&path, &settings).unwrap();
        let loaded = load_from(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.ui_theme, "dark");
        assert_eq!(loaded.board_theme, "glacier");
        assert_eq!(loaded.piece_set, "rhosgfx");
        assert_eq!(loaded.analysis_quality, "deep");
        assert_eq!(loaded.sfx_volume, 0.73);
        assert!(!loaded.custom_use_time_limit);
        assert_eq!(loaded.custom_think_time_ms, 2500);
        assert_eq!(loaded.custom_max_depth, 9);
    }

    /// A file missing keys this version added (or saved by an older
    /// version) must still load — serde's `#[serde(default)]` filling in
    /// anything absent is what lets a new setting ship with no migration.
    #[test]
    fn partial_file_fills_missing_fields_from_defaults() {
        let path = std::env::temp_dir().join("focalors_settings_test_partial.toml");
        std::fs::write(&path, "ui_theme = \"dark\"\n").unwrap();
        let loaded = load_from(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.ui_theme, "dark");
        assert_eq!(loaded.board_theme, Settings::default().board_theme);
        assert_eq!(loaded.sfx_volume, Settings::default().sfx_volume);
    }
}
