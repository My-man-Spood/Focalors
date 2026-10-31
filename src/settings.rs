//! User preferences, persisted as a TOML file next to the SQLite database
//! rather than as columns on it (see docs/REFACTORING.md item 4). A
//! preference is read once at startup and written one at a time when the
//! user changes it — not relational data, so it doesn't need a database.
//!
//! Field shapes intentionally match what the old `user_profile` columns
//! held (plain strings for the enum-backed ones), so the GUI's existing
//! `from_str`/`as_str` round-trip on each enum needs no changes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// The local-game setup card's last choices (side, time control,
    /// difficulty) — never persisted before this field existed, unlike
    /// the others above, so there's no old DB column to match.
    pub local_side_choice: String,
    pub local_time_preset: String,
    pub local_difficulty: String,
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
            local_side_choice: "white".to_string(),
            local_time_preset: "rapid_10_0".to_string(),
            local_difficulty: "tournament".to_string(),
        }
    }
}

pub(crate) fn settings_path() -> PathBuf {
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
/// the same way the old DB writes did — just keep running with the
/// in-memory value, and try again on the next change. `reason` is a short
/// tag identifying which setter triggered this (e.g. `"ui_theme"`,
/// `"begin_local_game"`) — logged alongside the write, part of the
/// diagnostic trail for the "settings keep resetting" bug (see the
/// conversation that added this).
pub fn save(settings: &Settings, reason: &str) -> std::io::Result<()> {
    save_to(&settings_path(), settings, reason)
}

// Path-parameterized so tests can round-trip through a temp file instead
// of the real settings_path() — exercising the real file I/O without any
// risk of clobbering the developer's own settings.toml when the test
// suite runs.

pub(crate) fn load_from(path: &std::path::Path) -> Settings {
    let settings = match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                log_settings_event(path, &format!(
                    "PARSE FAILURE, falling back to defaults: {e}\n--- raw content ---\n{text}\n---"
                ));
                Settings::default()
            }
        },
        // A missing file is the expected, quiet case on a fresh install —
        // only a file that exists but can't be *read* is worth logging.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            log_settings_event(path, &format!("UNREADABLE, falling back to defaults: {e}"));
            Settings::default()
        }
    };
    log_settings_event(
        path,
        &format!("load() -> ui_theme={:?} board_theme={:?}", settings.ui_theme, settings.board_theme),
    );
    settings
}

/// Same diagnostic trail as `log_settings_event`, for callers outside this
/// module that don't have a settings path handy (e.g. logging how many
/// times `FocalorsApp::new()` or `Database::open()` ran in one process —
/// prime suspect once the per-save logging showed `up()` itself firing
/// repeatedly despite the "already applied" guard looking correct).
pub(crate) fn log_event(message: &str) {
    log_settings_event(&settings_path(), message);
}

/// Diagnostic trail for the "settings keep resetting" bug (see the
/// conversation that added this): appends a timestamped line to
/// `settings_errors.log` next to `settings.toml`, and to stderr. Temporary
/// in spirit — once the actual cause is confirmed and fixed, this can be
/// trimmed back to just the genuine error paths it started as.
fn log_settings_event(settings_path: &std::path::Path, message: &str) {
    eprintln!("focalors settings [{}]: {message}", std::process::id());
    let Some(parent) = settings_path.parent() else { return };
    let log_path = parent.join("settings_errors.log");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{now}] pid={} {message}\n", std::process::id());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

/// Write via a temp file + rename, not a direct `fs::write`, so a process
/// killed mid-save (a crash, a force-quit, a WM killing a hung window)
/// never leaves `settings.toml` truncated. That used to be a real failure
/// mode: an empty file is syntactically valid TOML, so `#[serde(default)]`
/// silently turned "interrupted write" into "every setting quietly reset to
/// default" with no error anywhere — indistinguishable from the settings
/// just vanishing. A rename on the same filesystem is atomic, so the real
/// path is always either fully the old content or fully the new content,
/// never in between. Same pattern `install-local.sh` already uses for the
/// binary itself.
pub(crate) fn save_to(path: &std::path::Path, settings: &Settings, reason: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let text = toml::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp_path = parent.join(format!(
        ".settings.toml.tmp.{}.{}",
        std::process::id(),
        SAVE_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&tmp_path, text)?;
    std::fs::rename(&tmp_path, path)?;
    log_settings_event(
        path,
        &format!(
            "save() reason={reason:?} -> ui_theme={:?} board_theme={:?} piece_set={:?} \
             analysis_quality={:?} local_side_choice={:?} local_time_preset={:?} local_difficulty={:?}",
            settings.ui_theme,
            settings.board_theme,
            settings.piece_set,
            settings.analysis_quality,
            settings.local_side_choice,
            settings.local_time_preset,
            settings.local_difficulty,
        ),
    );
    Ok(())
}

/// Per-process counter so two saves racing on different threads (the app
/// never does this today, but the test suite's parallel runner does — all
/// tests share one process, so `process::id()` alone collides) never pick
/// the same temp filename.
static SAVE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        assert_eq!(s.local_side_choice, "white");
        assert_eq!(s.local_time_preset, "rapid_10_0");
        assert_eq!(s.local_difficulty, "tournament");
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let path = std::env::temp_dir().join("focalors_settings_test_no_tmp_leftover.toml");
        let _ = std::fs::remove_file(&path);
        save_to(&path, &Settings::default(), "test").unwrap();
        assert!(path.exists());
        let tmp_prefix = format!(".settings.toml.tmp.{}", std::process::id());
        let leftover = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(&tmp_prefix));
        assert!(!leftover, "temp file from save_to was not cleaned up by rename");
        let _ = std::fs::remove_file(&path);
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
            local_side_choice: "black".to_string(),
            local_time_preset: "blitz_5_0".to_string(),
            local_difficulty: "master".to_string(),
        };
        save_to(&path, &settings, "test").unwrap();
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
        assert_eq!(loaded.local_side_choice, "black");
        assert_eq!(loaded.local_time_preset, "blitz_5_0");
        assert_eq!(loaded.local_difficulty, "master");
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
