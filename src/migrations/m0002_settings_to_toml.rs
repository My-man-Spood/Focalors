//! Moves user preferences out of `user_profile` and into `settings.toml`
//! (see docs/REFACTORING.md item 4) — they're not relational data, they're
//! a handful of named values read once at startup and written one at a
//! time on change, which doesn't need a database.

use super::Migration;
use rusqlite::{Connection, Result as SqlResult};

pub struct M;

impl Migration for M {
    fn version(&self) -> i64 {
        2
    }

    fn name(&self) -> &str {
        "settings_to_toml"
    }

    fn up(&self, conn: &Connection, settings_path: &std::path::Path) -> SqlResult<()> {
        let settings = conn.query_row(
            "SELECT ui_theme, board_theme, piece_set, analysis_quality, sfx_volume,
                    custom_use_time_limit, custom_think_time_ms, custom_max_depth
             FROM user_profile WHERE id = 1",
            [],
            |row| {
                Ok(crate::settings::Settings {
                    ui_theme: row.get(0)?,
                    board_theme: row.get(1)?,
                    piece_set: row.get(2)?,
                    analysis_quality: row.get(3)?,
                    sfx_volume: row.get(4)?,
                    custom_use_time_limit: row.get::<_, i64>(5)? != 0,
                    custom_think_time_ms: row.get(6)?,
                    custom_max_depth: row.get(7)?,
                    // These were never DB columns (added after this
                    // migration was written) — the migration's job is
                    // only to carry over what the DB actually held.
                    ..crate::settings::Settings::default()
                })
            },
        )?;

        // Best-effort: a write failure here shouldn't block the schema
        // change. Worst case, the next load() falls back to defaults —
        // same failure mode the old DB-column reads already had.
        // `settings_path` (not the bare, hardcoded-real `settings::save`)
        // is what lets a test exercise this migration against a throwaway
        // path instead of silently overwriting the real settings.toml —
        // see the Migration trait's doc comment for why that matters.
        let _ = crate::settings::save_to(settings_path, &settings, "migration_m0002_up");

        conn.execute_batch(
            "
            ALTER TABLE user_profile DROP COLUMN ui_theme;
            ALTER TABLE user_profile DROP COLUMN board_theme;
            ALTER TABLE user_profile DROP COLUMN piece_set;
            ALTER TABLE user_profile DROP COLUMN analysis_quality;
            ALTER TABLE user_profile DROP COLUMN sfx_volume;
            ALTER TABLE user_profile DROP COLUMN custom_use_time_limit;
            ALTER TABLE user_profile DROP COLUMN custom_think_time_ms;
            ALTER TABLE user_profile DROP COLUMN custom_max_depth;
            ",
        )
    }

    fn down(&self, conn: &Connection, settings_path: &std::path::Path) -> SqlResult<()> {
        conn.execute_batch(
            "
            ALTER TABLE user_profile ADD COLUMN ui_theme TEXT NOT NULL DEFAULT 'light';
            ALTER TABLE user_profile ADD COLUMN board_theme TEXT NOT NULL DEFAULT 'walnut';
            ALTER TABLE user_profile ADD COLUMN piece_set TEXT NOT NULL DEFAULT 'cburnett';
            ALTER TABLE user_profile ADD COLUMN analysis_quality TEXT NOT NULL DEFAULT 'standard';
            ALTER TABLE user_profile ADD COLUMN sfx_volume REAL NOT NULL DEFAULT 0.2;
            ALTER TABLE user_profile ADD COLUMN custom_use_time_limit INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE user_profile ADD COLUMN custom_think_time_ms INTEGER NOT NULL DEFAULT 5000;
            ALTER TABLE user_profile ADD COLUMN custom_max_depth INTEGER NOT NULL DEFAULT 12;
            ",
        )?;

        let settings = crate::settings::load_from(settings_path);
        conn.execute(
            "UPDATE user_profile SET ui_theme = ?1, board_theme = ?2, piece_set = ?3,
                analysis_quality = ?4, sfx_volume = ?5, custom_use_time_limit = ?6,
                custom_think_time_ms = ?7, custom_max_depth = ?8
             WHERE id = 1",
            rusqlite::params![
                settings.ui_theme,
                settings.board_theme,
                settings.piece_set,
                settings.analysis_quality,
                settings.sfx_volume,
                settings.custom_use_time_limit as i64,
                settings.custom_think_time_ms,
                settings.custom_max_depth,
            ],
        )?;
        Ok(())
    }
}
