//! SQLite schema migrations. Replaces the old pattern of `ALTER TABLE ...
//! ADD COLUMN` calls run unconditionally on every startup with errors
//! swallowed (see docs/REFACTORING.md item 4) — each change is now a
//! permanent, numbered, tracked file instead of a script that ran once
//! and is easy to lose track of.
//!
//! Add a new migration by creating `mNNNN_description.rs`, implementing
//! `Migration` on a unit struct, and registering it in `registry()` below
//! in version order. Versions are plain sequential integers: this is a
//! solo-developer project on one branch, so there's no multi-contributor
//! merge-collision risk that timestamp-based versioning exists to solve.

use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;

mod m0001_initial_schema;
mod m0002_settings_to_toml;
mod m0003_persist_move_pv;

pub trait Migration {
    fn version(&self) -> i64;
    fn name(&self) -> &str;
    /// `settings_path` is only there for the one migration that touches
    /// settings.toml (m0002) — most migrations ignore it. Threading it
    /// through explicitly, rather than having migrations reach for the
    /// real on-disk path themselves, is what lets a test exercise the full
    /// migration chain (e.g. `db.rs`'s tests, which call `run_pending` on
    /// a throwaway in-memory DB just to get the schema) without silently
    /// overwriting the developer's actual settings.toml — which is
    /// exactly what happened before this parameter existed: every
    /// `cargo test` run re-triggered m0002 against a fresh in-memory DB
    /// and it dutifully wrote real defaults to the real file.
    fn up(&self, conn: &Connection, settings_path: &Path) -> SqlResult<()>;
    /// Reverses `up`. Never called automatically by the app — exists for
    /// manual/dev use, via `rollback_last` below.
    #[allow(dead_code)]
    fn down(&self, conn: &Connection, settings_path: &Path) -> SqlResult<()>;
}

fn registry() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(m0001_initial_schema::M),
        Box::new(m0002_settings_to_toml::M),
        Box::new(m0003_persist_move_pv::M),
    ]
}

fn ensure_tracking_table(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
}

fn is_applied(conn: &Connection, version: i64) -> SqlResult<bool> {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = ?1",
        params![version],
        |row| row.get(0),
    )
}

/// Runs every registered migration not yet recorded in
/// `schema_migrations`, in version order, each inside its own
/// transaction — so a failed migration can't leave the schema half
/// changed, and a migration that already succeeded is never re-run.
pub fn run_pending(conn: &Connection, settings_path: &Path) -> SqlResult<()> {
    run_migrations(conn, registry(), true, settings_path)
}

/// Same as `run_pending`, with diagnostic logging off — for tests outside
/// this module (`db.rs`'s, which need the real migrations' schema but have
/// no business writing to the real settings_errors.log). `settings_path`
/// alone already keeps the *settings* side test-safe; this covers the
/// separate diagnostic-log side, which `log_event` always writes to the
/// real path regardless of what `settings_path` says.
#[cfg(test)]
pub(crate) fn run_pending_for_test(conn: &Connection, settings_path: &Path) -> SqlResult<()> {
    run_migrations(conn, registry(), false, settings_path)
}

// Takes the migration list as a parameter (rather than calling registry()
// directly) so tests can exercise the tracking/skip/rollback mechanics
// with throwaway fake migrations — real migrations touch real state
// (m0002 writes settings.toml), which a test has no business doing.
// `log` is false from tests for the same reason: the diagnostic logging
// below writes to the real settings path, which a test must never touch.
fn run_migrations(
    conn: &Connection,
    mut migrations: Vec<Box<dyn Migration>>,
    log: bool,
    settings_path: &Path,
) -> SqlResult<()> {
    ensure_tracking_table(conn)?;

    migrations.sort_by_key(|m| m.version());

    for migration in &migrations {
        let already = is_applied(conn, migration.version())?;
        // Diagnostic for the "settings keep resetting" investigation:
        // m0002's own save() logging showed it firing repeatedly despite
        // this guard — this line shows directly whether is_applied() is
        // the one lying, or something else is calling up() unguarded.
        if log {
            crate::settings::log_event(&format!(
                "migration v{} ({}): already_applied={already}",
                migration.version(),
                migration.name(),
            ));
        }
        if already {
            continue;
        }

        conn.execute_batch("BEGIN")?;
        let result = migration.up(conn, settings_path).and_then(|()| {
            conn.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version(), migration.name()],
            )?;
            Ok(())
        });
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                if log {
                    crate::settings::log_event(&format!(
                        "migration v{} FAILED, rolling back: {e}",
                        migration.version()
                    ));
                }
                conn.execute_batch("ROLLBACK")?;
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Rolls back the most recently applied migration. Not called anywhere in
/// the app's own startup path — a manual/dev tool for when a migration
/// needs undoing locally.
#[allow(dead_code)]
pub fn rollback_last(conn: &Connection, settings_path: &Path) -> SqlResult<()> {
    let last: Option<i64> = conn
        .query_row(
            "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(version) = last else {
        return Ok(());
    };

    let migrations = registry();
    let Some(migration) = migrations.iter().find(|m| m.version() == version) else {
        return Ok(());
    };

    conn.execute_batch("BEGIN")?;
    let result = migration.down(conn, settings_path).and_then(|()| {
        conn.execute(
            "DELETE FROM schema_migrations WHERE version = ?1",
            params![version],
        )?;
        Ok(())
    });
    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            conn.execute_batch("ROLLBACK")?;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingMigration {
        version: i64,
        up_calls: &'static AtomicUsize,
    }

    impl Migration for CountingMigration {
        fn version(&self) -> i64 {
            self.version
        }
        fn name(&self) -> &str {
            "counting_migration"
        }
        fn up(&self, conn: &Connection, _settings_path: &Path) -> SqlResult<()> {
            self.up_calls.fetch_add(1, Ordering::SeqCst);
            conn.execute_batch(&format!(
                "CREATE TABLE t{} (id INTEGER PRIMARY KEY)",
                self.version
            ))
        }
        fn down(&self, conn: &Connection, _settings_path: &Path) -> SqlResult<()> {
            conn.execute_batch(&format!("DROP TABLE t{}", self.version))
        }
    }

    struct FailingMigration {
        version: i64,
    }

    impl Migration for FailingMigration {
        fn version(&self) -> i64 {
            self.version
        }
        fn name(&self) -> &str {
            "failing_migration"
        }
        fn up(&self, conn: &Connection, _settings_path: &Path) -> SqlResult<()> {
            // Create a table, then fail — up's own partial work must roll
            // back along with the schema_migrations insert.
            conn.execute_batch("CREATE TABLE partial (id INTEGER)")?;
            conn.execute_batch("INSERT INTO a_table_that_does_not_exist (id) VALUES (1)")?;
            Ok(())
        }
        fn down(&self, _conn: &Connection, _settings_path: &Path) -> SqlResult<()> {
            Ok(())
        }
    }

    #[test]
    fn applies_pending_migrations_in_version_order_and_records_them() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let conn = Connection::open_in_memory().unwrap();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(CountingMigration { version: 2, up_calls: &CALLS }),
            Box::new(CountingMigration { version: 1, up_calls: &CALLS }),
        ];
        run_migrations(&conn, migrations, false, Path::new("unused")).unwrap();

        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        // Both tables exist — version order didn't skip either.
        conn.execute_batch("INSERT INTO t1 DEFAULT VALUES; INSERT INTO t2 DEFAULT VALUES;")
            .unwrap();

        let recorded: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<SqlResult<_>>()
            .unwrap();
        assert_eq!(recorded, vec![1, 2]);
    }

    #[test]
    fn already_applied_migrations_are_never_re_run() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let conn = Connection::open_in_memory().unwrap();
        let make = || -> Vec<Box<dyn Migration>> {
            vec![Box::new(CountingMigration { version: 1, up_calls: &CALLS })]
        };

        run_migrations(&conn, make(), false, Path::new("unused")).unwrap();
        run_migrations(&conn, make(), false, Path::new("unused")).unwrap();

        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "up() must run exactly once");
    }

    #[test]
    fn a_failed_migration_rolls_back_and_is_not_recorded() {
        let conn = Connection::open_in_memory().unwrap();
        let migrations: Vec<Box<dyn Migration>> = vec![Box::new(FailingMigration { version: 1 })];

        assert!(run_migrations(&conn, migrations, false, Path::new("unused")).is_err());

        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(applied, 0, "a failed migration must not be recorded as applied");

        // The partial work inside the failed migration's own up() must
        // have rolled back too, not just the schema_migrations insert.
        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='partial'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_exists, 0, "the failed migration's partial work must roll back");
    }

    #[test]
    fn down_reverses_what_up_did() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let conn = Connection::open_in_memory().unwrap();
        let migration = CountingMigration { version: 1, up_calls: &CALLS };
        migration.up(&conn, Path::new("unused")).unwrap();
        migration.down(&conn, Path::new("unused")).unwrap();

        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='t1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_exists, 0);
    }
}
