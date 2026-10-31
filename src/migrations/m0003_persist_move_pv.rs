//! Adds a column to store each analyzed move's engine principal variation
//! (the continuation the engine expects from that position), so Game
//! Review's "what happens next" line survives a restart instead of only
//! existing for the few minutes after a fresh analysis run. Space-separated
//! UCI moves rather than a second table — it's an ordered list belonging to
//! exactly one `move_analysis` row, not relational data of its own.

use super::Migration;
use rusqlite::{Connection, Result as SqlResult};

pub struct M;

impl Migration for M {
    fn version(&self) -> i64 {
        3
    }

    fn name(&self) -> &str {
        "persist_move_pv"
    }

    fn up(&self, conn: &Connection, _settings_path: &std::path::Path) -> SqlResult<()> {
        conn.execute_batch(
            "ALTER TABLE move_analysis ADD COLUMN best_line TEXT NOT NULL DEFAULT '';",
        )
    }

    fn down(&self, conn: &Connection, _settings_path: &std::path::Path) -> SqlResult<()> {
        conn.execute_batch("ALTER TABLE move_analysis DROP COLUMN best_line;")
    }
}
