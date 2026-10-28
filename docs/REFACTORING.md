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

- [ ] **3. Centralize duplicated rule helpers.** "Is this move a capture"
  is implemented separately in `db.rs`, `selfplay.rs`, and `gui.rs`. One
  shared helper in `board.rs` or `moves.rs`, called from all three.

- [ ] **4. Pull settings out of the `user_profile` SQL table.** Every
  preference is a column bolted on via `ALTER TABLE ADD COLUMN ...
  DEFAULT x` with errors swallowed. No schema version, no down-migration,
  and settings are mixed into the same row as profile stats. Move to a
  real key-value settings table (or a TOML prefs file) and leave SQLite
  for game records.

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
