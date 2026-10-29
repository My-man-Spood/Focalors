//! The schema as it stood right before this migration system existed —
//! everything the old `init_schema()` plus the old `migrate_phase4()`'s
//! pile of `ALTER TABLE ADD COLUMN` calls had already produced on disk.
//!
//! On an existing database every statement here is a no-op (the tables
//! already exist with these columns, from that old system) — this just
//! gets recorded as applied. On a brand-new install, this is what
//! actually creates everything, in one place, instead of a base schema
//! plus a trail of ALTER statements layered on top of it.

use super::Migration;
use rusqlite::{Connection, Result as SqlResult};

pub struct M;

impl Migration for M {
    fn version(&self) -> i64 {
        1
    }

    fn name(&self) -> &str {
        "initial_schema"
    }

    fn up(&self, conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS user_profile (
                id                     INTEGER PRIMARY KEY DEFAULT 1,
                name                   TEXT    NOT NULL DEFAULT 'Player',
                rating                 INTEGER NOT NULL DEFAULT 1200,
                games_played           INTEGER NOT NULL DEFAULT 0,
                created_at             TEXT    NOT NULL DEFAULT (datetime('now')),
                updated_at             TEXT    NOT NULL DEFAULT (datetime('now')),
                rating_games           INTEGER NOT NULL DEFAULT 0,
                adaptive_level         INTEGER NOT NULL DEFAULT 10,
                ui_theme               TEXT    NOT NULL DEFAULT 'light',
                board_theme            TEXT    NOT NULL DEFAULT 'walnut',
                piece_set              TEXT    NOT NULL DEFAULT 'cburnett',
                analysis_quality       TEXT    NOT NULL DEFAULT 'standard',
                sfx_volume             REAL    NOT NULL DEFAULT 0.2,
                custom_use_time_limit  INTEGER NOT NULL DEFAULT 1,
                custom_think_time_ms   INTEGER NOT NULL DEFAULT 5000,
                custom_max_depth       INTEGER NOT NULL DEFAULT 12
            );

            CREATE TABLE IF NOT EXISTS games (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                played_at             TEXT    NOT NULL DEFAULT (datetime('now')),
                user_color            TEXT    NOT NULL CHECK(user_color IN ('white', 'black')),
                result                TEXT    NOT NULL CHECK(result IN ('win', 'loss', 'draw')),
                result_reason         TEXT,
                time_control          TEXT,
                engine_level          TEXT,
                pgn                   TEXT    NOT NULL,
                move_count            INTEGER,
                user_accuracy         REAL,
                opening_name          TEXT,
                engine_numeric_level  INTEGER,
                rating_before         INTEGER,
                rating_after          INTEGER
            );

            CREATE TABLE IF NOT EXISTS move_analysis (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                game_id         INTEGER NOT NULL REFERENCES games(id),
                move_number     INTEGER NOT NULL,
                side            TEXT    NOT NULL CHECK(side IN ('white', 'black')),
                move_uci        TEXT    NOT NULL,
                move_san        TEXT,
                eval_before     INTEGER,
                eval_after      INTEGER,
                best_move       TEXT,
                best_eval       INTEGER,
                classification  TEXT,
                eval_components TEXT
            );

            CREATE TABLE IF NOT EXISTS puzzles (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                game_id    INTEGER REFERENCES games(id),
                fen        TEXT    NOT NULL,
                solution   TEXT    NOT NULL,
                theme      TEXT,
                rating     INTEGER,
                attempts   INTEGER DEFAULT 0,
                solved     INTEGER DEFAULT 0,
                created_at TEXT    NOT NULL DEFAULT (datetime('now'))
            );

            -- Later migrations (and later code in general) assume row
            -- id=1 exists. On an existing install it already does, so this
            -- is a no-op; on a brand-new install it otherwise wouldn't
            -- exist until `get_or_create_profile()` runs, which happens
            -- after `Database::open()` — after migrations have already
            -- run.
            INSERT OR IGNORE INTO user_profile (id, name, rating, games_played)
                VALUES (1, 'Player', 1200, 0);
            ",
        )
    }

    fn down(&self, conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            DROP TABLE IF EXISTS puzzles;
            DROP TABLE IF EXISTS move_analysis;
            DROP TABLE IF EXISTS games;
            DROP TABLE IF EXISTS user_profile;
            ",
        )
    }
}
