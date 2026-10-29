# Refactoring roadmap

Tracked tech debt from an architecture review, roughly ranked by value vs.
effort. Check items off as they land; add new ones the same way.

- [x] **1. Split `src/gui.rs` into page modules.** Done. `gui.rs`
  (8,482 lines, one `impl FocalorsApp` mixing rendering, game rules,
  persistence, and audio) is now `src/gui/` with `mod.rs` down to 2,132
  lines (core types, `FocalorsApp`, the `eframe::App` impl) plus:
  `home.rs`, `play.rs`, `review.rs`, `puzzle.rs`, `history.rs`,
  `settings.rs` (page-specific `impl FocalorsApp` blocks), and the
  stateless helper modules `style.rs`, `board_render.rs`, `game_state.rs`.
  Every moved method/function is `pub(crate)` (method privacy is scoped to
  where the `impl` block textually lives, not where the type is defined,
  so cross-page calls like `mod.rs`'s `ui()` need it). Verified
  mechanically: scripted the partition, diffed every function signature
  before/after to confirm none were lost or duplicated, then full
  `cargo build` + `cargo test` (203 passed) + `cargo clippy` (zero new
  warnings) before and after.

- [x] **2. Collapse `pending_sounds` into `pending_animations`.** Done —
  turned out leaner than the originally-planned generic channel/enum.
  `pending_animations` and `pending_sounds` were two separate hand-rolled
  queues (one `Vec` per message type, both drained by `draw_board`). The
  move/capture sound decision only needs the before-move board, which is
  exactly what's already available at the moment a `PendingAnimation` gets
  built — so `PendingAnimation` grew two fields (`captured: bool`,
  `animate: bool`) instead of needing a second queue. `animate` matters
  because a drag-drop move skips the glide (the piece is already drawn at
  its destination) but still needs its sound, so the two concerns can't be
  conflated into "push only when animating." The one sound that genuinely
  isn't tied to a move — game-over by resignation or timeout, which has no
  animation at all — stays a direct state check in `draw_board` (did
  `local_game.outcome` flip to `Some` since last frame), never a queue.
  A generic `mpsc`/`GuiEvent` channel is still the right call *if* a third,
  genuinely independent kind of cross-thread message shows up later; this
  one didn't need that machinery.

- [x] **3. Centralize duplicated rule helpers.** Done — and it turned out
  to be four copies, not three: `db.rs` (SAN generation), `selfplay.rs`
  (training-data generation), `gui/game_state.rs` (the sound/animation
  work), and a fourth in `search.rs`'s hot move-ordering path that nobody
  had flagged, written as a faster bitboard check
  (`color_bb(enemy).contains(sq)`) instead of the `piece_on(sq)` the other
  three used — same result, cheaper, since it skips identifying *which*
  piece is there. Collapsed into one method, `Move::is_capture(self,
  board: &Board) -> bool` in `moves.rs`, using the fast bitboard version as
  the canonical implementation so the search path keeps its performance
  instead of inheriting the slower check. All four call sites now read
  `mv.is_capture(&board)`. Verified: full build, full test suite (203
  passed, including the SEE/capture-specific search tests), clippy diff
  showed no new warnings.

- [x] **4. Pull settings out of the `user_profile` SQL table.** Done, and
  grew into two things: a real migration framework for SQLite, and
  `settings.toml` for preferences.

  **Migration framework** (`src/migrations/`): a `Migration` trait
  (`version`, `name`, `up`, `down`), one file per migration
  (`m0001_initial_schema.rs`, `m0002_settings_to_toml.rs`), a
  `schema_migrations` tracking table, and a runner that applies whatever
  isn't yet recorded, each inside its own transaction. Plain sequential
  version numbers, not timestamps — this is a solo project on one branch,
  so there's no multi-contributor merge-collision risk to design around.
  `down()` is real and tested but never auto-invoked; it exists for
  manual/dev rollback.

  Migration #1 is the schema as it stood right before this system
  existed — everything the old `init_schema()` plus the old
  `migrate_phase4()`'s pile of `ALTER TABLE ADD COLUMN` calls had already
  produced. A no-op on every existing install (tables already exist), the
  real thing on a fresh one. No attempt to retroactively "replay" history
  that already safely happened on every machine that's ever run the app —
  real tracked migrations start from here, going forward.

  Migration #2 is the actual change: reads the 8 settings columns out of
  `user_profile`, writes `settings.toml`, drops the columns.

  **Settings file** (`src/settings.rs`): a `Settings` struct
  (`serde` + `toml`), loaded once at startup instead of 6 separate DB
  round trips, saved as a whole file on every change (TOML's idiomatic
  shape — no `UPDATE ... SET x = ?` equivalent, and that's fine: a new
  setting just gets `#[serde(default)]`, no migration needed at all; a
  removed one just goes unwritten next save, serde ignores unknown keys
  on load). Field shapes match what the DB columns held, so the GUI's
  existing `from_db_str`/`as_db_str` conversions needed no changes.

  Verified: full build, full test suite (208 passed — 3 DB tests for the
  removed settings columns deleted, replaced with 8 new tests: 4 for
  `settings.rs`'s load/save/default/partial-file behavior using temp
  files rather than the real settings path, 4 for the migration runner's
  skip/rollback/ordering mechanics using throwaway fake migrations rather
  than the real ones — both deliberately avoid touching real on-disk
  state from a test run). Then verified for real: ran the actual binary
  against an isolated copy of the real, in-use database (never the real
  file) — confirmed the 8 columns dropped, `settings.toml` came out with
  the exact values the database had, a second run didn't re-apply
  anything, and a from-scratch fresh install built correctly with no
  dangling settings columns.

- [ ] **5. Workspace split: engine lib vs. GUI bin.** The engine
  (`board`/`movegen`/`attacks`/`moves`/`eval`/`search`/`nnue`) is already
  GUI-agnostic in practice; it just isn't packaged that way. A
  `focalors-engine` lib crate + `focalors-gui` bin would let the engine be
  reused (e.g. a bare UCI binary) without pulling in `eframe`/`egui`/
  `rodio`/`rusqlite`. Biggest structural win, also the most work — lowest
  urgency for a single-user app.

Also noted, not yet ranked:
- `selfplay`/`selfmatch`/`tune`/`train`/`uci` ship in the same binary and
  dependency graph as the end-user GUI; `gpu-training` got a Cargo feature
  flag to keep Burn out of the shipped binary, the rest of the dev tooling
  didn't.
- No documented lock-ordering invariant for `state` vs.
  `persistent_searcher`, despite the code already being careful about it.
  53 `.lock().unwrap()` sites share one `Mutex`; a panic on either
  background search thread while it's held poisons all of them.
- Inconsistent error-handling posture across the codebase (silent
  `let _ = db.x()`, `eprintln!` warnings, `unwrap()`/`panic!`) with no
  stated rule for which applies where.
