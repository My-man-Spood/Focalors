use super::*;

/// Build the glide-and-sound payload for `mv` from the board it is about to
/// be played on. `None` for null moves or an unexpectedly empty origin
/// square. `animate` is false for drag-drops (the piece is already on its
/// destination square, so a glide would visually replay the move) — the
/// payload still carries `captured` either way, since the sound fires
/// regardless of whether anything visually glides.
pub(crate) fn pending_animation(board: &Board, mv: Move, animate: bool) -> Option<PendingAnimation> {
    if mv.is_null() {
        return None;
    }
    let piece = board.piece_on(mv.from_sq())?;
    let (from_sq, to_sq) = (mv.from_sq().0, mv.to_sq().0);
    // Castling is encoded as the king's move (e1g1 / e1c1); derive the
    // rook's path from the king's destination file.
    let rook = match mv.flag() {
        MoveFlag::Castling if to_sq % 8 == 6 => Some((to_sq + 1, to_sq - 1)),
        MoveFlag::Castling => Some((to_sq - 2, to_sq + 1)),
        _ => None,
    };
    let captured = mv.is_capture(board);
    Some(PendingAnimation { from_sq, to_sq, piece, rook, captured, animate })
}

/// SFX for a move that just landed, lichess-style: no distinct "check"
/// sound (the move/capture sound alone covers it — matching lichess's own
/// default theme, where Check.mp3 is silence).
pub(crate) fn move_sfx(captured: bool) -> crate::sound::Sfx {
    if captured { crate::sound::Sfx::Capture } else { crate::sound::Sfx::Move }
}

/// Read-only view of a board position, used by `draw_board` when rendering
/// something other than the live game (e.g. replay).

/// Replay the saved PGN once, building parallel `boards`/`moves` vectors so the
/// review UI can step through positions without re-parsing on every frame.
/// Falls back to a start-position-only state if the PGN is unparseable.
pub(crate) fn build_replay_state(game: crate::db::SavedGame) -> ReplayState {
    let parsed = crate::pgn::parse_pgn(&game.pgn).ok();
    let mut board = Board::startpos();
    let mut boards = vec![board.clone()];
    let mut moves = Vec::new();
    let mut uci_moves = Vec::new();

    if let Some(p) = parsed {
        for uci in p.uci_moves {
            let mv = match crate::uci::parse_move(&board, &uci) {
                Some(m) => m,
                None => break,
            };
            make_move(&mut board, mv);
            uci_moves.push(uci);
            moves.push(mv);
            boards.push(board.clone());
        }
    }

    ReplayState {
        game,
        uci_moves,
        boards,
        moves,
        cursor: 0,
    }
}

/// The twelve piece images of one set, embedded from `assets/pieces/<dir>/`:
/// (side, piece, texture name, PNG bytes).

/// Parse the from/to squares of a UCI move ("e2e4", "e7e8q") into square
/// indices for drawing a Game Review arrow. Returns `None` for malformed or
/// empty UCI (UCI is always ASCII, so byte slicing is safe).
/// Drops a puzzle session and gives the shared board back to the start
/// position, unless a game owns it. Returns whether there was a session.
pub(crate) fn end_puzzle_session(
    trainer: &mut Option<PuzzleTrainerState>,
    message: &mut Option<(String, egui::Color32, std::time::Instant)>,
    state: &mut SharedState,
) -> bool {
    if trainer.take().is_none() {
        return false;
    }
    *message = None;
    if !state.local_game.active {
        state.board = Board::startpos();
    }
    true
}

/// Squares of the move that produced the displayed live position, so the
/// live board tints them the way Game Review does. Follows the history
/// cursor: stepping back shows the move that led to that snapshot. None
/// outside a local game (a puzzle starts from a position whose previous
/// move is unknown) and at the start position.
pub(crate) fn live_last_move_squares(state: &SharedState) -> Option<(u8, u8)> {
    if !state.local_game.active {
        return None;
    }
    state
        .local_history
        .get(state.local_history_cursor)
        .and_then(|snapshot| snapshot.move_uci.as_deref())
        .and_then(uci_arrow_squares)
}

pub(crate) fn seed_local_history(state: &mut SharedState) {
    state.local_history.clear();
    state.local_history.push(LocalSnapshot {
        board: state.board.clone(),
        white_remaining_ms: state.local_game.time_control.white_remaining_ms,
        black_remaining_ms: state.local_game.time_control.black_remaining_ms,
        move_uci: None,
    });
    state.local_history_cursor = 0;
    sync_local_move_history(state);
}

pub(crate) fn sync_local_move_history(state: &mut SharedState) {
    state.move_history = state
        .local_history
        .iter()
        .take(state.local_history_cursor.saturating_add(1))
        .skip(1)
        .filter_map(|snapshot| snapshot.move_uci.clone())
        .collect();
}

pub(crate) fn record_local_snapshot(state: &mut SharedState, move_uci: String) {
    if state.local_history.is_empty() {
        seed_local_history(state);
    }

    let truncate_len = state.local_history_cursor.saturating_add(1);
    if truncate_len < state.local_history.len() {
        state.local_history.truncate(truncate_len);
    }

    state.local_search_generation = state.local_search_generation.wrapping_add(1);
    state.local_history.push(LocalSnapshot {
        board: state.board.clone(),
        white_remaining_ms: state.local_game.time_control.white_remaining_ms,
        black_remaining_ms: state.local_game.time_control.black_remaining_ms,
        move_uci: Some(move_uci),
    });
    state.local_history_cursor = state.local_history.len().saturating_sub(1);
    sync_local_move_history(state);
}

pub(crate) fn restore_local_snapshot(state: &mut SharedState, snapshot: &LocalSnapshot) {
    state.board = snapshot.board.clone();
    state.local_game.time_control.white_remaining_ms = snapshot.white_remaining_ms;
    state.local_game.time_control.black_remaining_ms = snapshot.black_remaining_ms;
    state.local_game.time_control.clear_active();
}

pub(crate) fn invalidate_local_search(state: &mut SharedState, clear_info: bool) {
    state.local_search_generation = state.local_search_generation.wrapping_add(1);
    if clear_info {
        state.search_info = SearchInfo::default();
    } else {
        state.search_info.searching = false;
    }
}

pub(crate) fn local_history_label(index: usize, move_uci: Option<&str>) -> String {
    match move_uci {
        None => "Start position".to_string(),
        Some(mv) if index % 2 == 1 => format!("{}. {}", (index + 1) / 2, mv),
        Some(mv) => format!("{}... {}", index / 2, mv),
    }
}

pub(crate) fn is_reviewing_history(state: &SharedState) -> bool {
    !state.local_history.is_empty() && state.local_history_cursor + 1 < state.local_history.len()
}

pub(crate) fn local_engine_search_request(
    state: &SharedState,
    strength_config: &crate::strength::StrengthConfig,
) -> Option<LocalSearchRequest> {
    if !state.local_game.active || state.local_game.outcome.is_some() {
        return None;
    }

    let side_to_move = state.board.side_to_move;
    let remaining_ms = state
        .local_game
        .time_control
        .displayed_remaining_ms(side_to_move, Some(side_to_move));
    let safe_max_ms = remaining_ms.saturating_sub(50).max(1);

    if state.local_game.difficulty == LocalDifficulty::Custom {
        let settings = &state.engine_settings;
        let (soft_time_ms, hard_time_ms, depth_cap) = if settings.use_time_limit {
            let hard = settings.think_time_ms.min(safe_max_ms);
            let soft = (hard / 2).max(1);
            (soft, hard, None)
        } else {
            (safe_max_ms, safe_max_ms, Some(settings.max_depth))
        };
        return Some(LocalSearchRequest {
            soft_time_ms,
            hard_time_ms,
            depth_cap,
            generation: state.local_search_generation,
            strength_config: strength_config.clone(),
        });
    }

    let (base_time_ms, _) = uci::allocate_time(
        remaining_ms,
        state.local_game.time_control.increment_ms,
        None,
    );
    let scaled_time_ms = ((base_time_ms as f32) * strength_config.time_scale).round() as u64;
    let min_time_ms = safe_max_ms.min(80);
    let hard_time_ms = scaled_time_ms
        .max(min_time_ms)
        .max(strength_config.min_think_ms)
        .min(safe_max_ms);
    let soft_time_ms = (hard_time_ms * 4 / 10).max(min_time_ms.min(hard_time_ms));

    Some(LocalSearchRequest {
        soft_time_ms,
        hard_time_ms,
        depth_cap: strength_config.max_depth,
        generation: state.local_search_generation,
        strength_config: strength_config.clone(),
    })
}

pub(crate) fn consume_local_turn_time(state: &mut SharedState, mover: Color) -> bool {
    if !state.local_game.active || state.local_game.outcome.is_some() {
        return false;
    }

    let remaining_ms = state.local_game.time_control.consume_running_time(mover);
    if remaining_ms == 0 {
        state.local_game.time_control.flag_side(mover);
        set_local_game_outcome(state, GameOutcome::Timeout(mover.flip()));
        return false;
    }

    true
}

pub(crate) fn handle_active_side_timeout(state: &mut SharedState) -> bool {
    if !state.local_game.active || state.local_game.outcome.is_some() {
        return false;
    }

    let active_side = state.board.side_to_move;
    let expired = state
        .local_game
        .time_control
        .displayed_remaining_ms(active_side, Some(active_side))
        == 0;

    if expired {
        state.local_game.time_control.flag_side(active_side);
        set_local_game_outcome(state, GameOutcome::Timeout(active_side.flip()));
        return true;
    }

    false
}

pub(crate) fn is_draw_by_threefold(state: &SharedState) -> bool {
    let Some(current_snapshot) = state.local_history.get(state.local_history_cursor) else {
        return false;
    };

    let current_hash = current_snapshot.board.hash;
    state
        .local_history
        .iter()
        .take(state.local_history_cursor.saturating_add(1))
        .filter(|snapshot| snapshot.board.hash == current_hash)
        .count()
        >= 3
}

pub(crate) fn detect_game_outcome(state: &SharedState) -> Option<GameOutcome> {
    let board = &state.board;
    let moves = generate_legal_moves(board);
    if moves.is_empty() {
        let king_sq = board.piece_bb(board.side_to_move, Piece::King).lsb();
        let in_check = attacks::is_square_attacked(board, king_sq, board.side_to_move.flip());
        return if in_check {
            Some(GameOutcome::Checkmate(board.side_to_move.flip()))
        } else {
            Some(GameOutcome::Stalemate)
        };
    }

    if board.is_insufficient_material() {
        return Some(GameOutcome::InsufficientMaterial);
    }

    if is_draw_by_threefold(state) {
        return Some(GameOutcome::ThreefoldRepetition);
    }

    if board.halfmove_clock >= 100 {
        return Some(GameOutcome::FiftyMoveRule);
    }

    None
}

pub(crate) fn local_outcome_banner(outcome: GameOutcome, human_color: Color) -> (String, egui::Color32) {
    match outcome {
        GameOutcome::Checkmate(winner) if winner == human_color => (
            "CHECKMATE - You win!".to_string(),
            egui::Color32::LIGHT_GREEN,
        ),
        GameOutcome::Checkmate(_) => (
            "CHECKMATE - Focalors wins".to_string(),
            egui::Color32::RED,
        ),
        GameOutcome::Stalemate => (
            "STALEMATE - Draw".to_string(),
            egui::Color32::YELLOW,
        ),
        GameOutcome::FiftyMoveRule => (
            "DRAW BY 50-MOVE RULE".to_string(),
            egui::Color32::YELLOW,
        ),
        GameOutcome::ThreefoldRepetition => (
            "DRAW BY REPETITION".to_string(),
            egui::Color32::YELLOW,
        ),
        GameOutcome::InsufficientMaterial => (
            "DRAW BY INSUFFICIENT MATERIAL".to_string(),
            egui::Color32::YELLOW,
        ),
        GameOutcome::Timeout(winner) if winner == human_color => (
            "TIMEOUT - You win!".to_string(),
            egui::Color32::LIGHT_GREEN,
        ),
        GameOutcome::Timeout(_) => (
            "TIMEOUT - Focalors wins".to_string(),
            egui::Color32::RED,
        ),
        GameOutcome::Resignation(winner) if winner == human_color => (
            "RESIGNATION - You win!".to_string(),
            egui::Color32::LIGHT_GREEN,
        ),
        GameOutcome::Resignation(_) => (
            "RESIGNATION - Focalors wins".to_string(),
            egui::Color32::RED,
        ),
    }
}

pub(crate) fn local_outcome_status(outcome: GameOutcome, human_color: Color) -> String {
    match outcome {
        GameOutcome::Checkmate(winner) if winner == human_color => {
            "Checkmate. You win!".to_string()
        }
        GameOutcome::Checkmate(_) => "Checkmate. Focalors wins.".to_string(),
        GameOutcome::Stalemate => "Draw by stalemate.".to_string(),
        GameOutcome::FiftyMoveRule => "Draw by 50-move rule.".to_string(),
        GameOutcome::ThreefoldRepetition => "Draw by threefold repetition.".to_string(),
        GameOutcome::InsufficientMaterial => {
            "Draw by insufficient material.".to_string()
        }
        GameOutcome::Timeout(winner) if winner == human_color => {
            "Focalors flagged. You win on time!".to_string()
        }
        GameOutcome::Timeout(_) => "You flagged. Focalors wins on time.".to_string(),
        GameOutcome::Resignation(winner) if winner == human_color => {
            "Focalors resigned. You win!".to_string()
        }
        GameOutcome::Resignation(_) => "You resigned. Focalors wins.".to_string(),
    }
}

pub(crate) fn set_local_game_outcome(state: &mut SharedState, outcome: GameOutcome) {
    invalidate_local_search(state, false);
    state.local_game.outcome = Some(outcome);
    state.local_game.time_control.clear_active();
    state.status_message = local_outcome_status(outcome, state.local_game.human_color);
}

pub(crate) fn update_local_game_outcome(state: &mut SharedState) {
    if !state.local_game.active || state.local_game.outcome.is_some() {
        return;
    }

    if let Some(outcome) = detect_game_outcome(state) {
        set_local_game_outcome(state, outcome);
    }
}

/// Format a White-POV centipawn score for display. Scores inside the mate
/// window (|cp| > 20000, i.e. near ±MATE_SCORE) render as "M{n}" / "-M{n}"
/// (full moves to mate) instead of an absurd pawn count; everything else as
/// signed pawns to two decimals. Shared by the live score readout and the
/// Game Review detail line.
pub(crate) fn format_eval(cp: crate::eval::Score) -> String {
    use crate::eval::MATE_DISPLAY_CUTOFF;
    if cp.abs() > MATE_DISPLAY_CUTOFF {
        let mate_in = (crate::eval::MATE_SCORE - cp.abs() + 1) / 2;
        if cp > 0 {
            format!("M{mate_in}")
        } else {
            format!("-M{mate_in}")
        }
    } else {
        format!("{:+.2}", cp as f64 / 100.0)
    }
}

pub(crate) fn chrono_today() -> String {
    // Simple date without pulling in the chrono crate
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Approximate: days since epoch
    let days = secs / 86400;
    // Zeller-like calculation for year/month/day
    let mut y = 1970i32;
    let mut remaining = days as i64;
    loop {
        let year_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < year_days { break; }
        remaining -= year_days;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    for md in month_days {
        if remaining < md as i64 { break; }
        remaining -= md as i64;
        m += 1;
    }
    format!("{y:04}.{:02}.{:02}", m + 1, remaining + 1)
}

