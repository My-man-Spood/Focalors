use super::*;

impl FocalorsApp {
    pub(crate) fn save_completed_game(&mut self) {
        let db = match &self.db {
            Some(d) => d,
            None => return,
        };

        let state = self.state.lock().unwrap();
        let local_game = &state.local_game;
        let outcome = match &local_game.outcome {
            Some(o) => o,
            None => return,
        };

        let human_color = local_game.human_color;
        let user_color = if human_color == Color::White { "white" } else { "black" };

        let (result, result_reason) = match outcome {
            GameOutcome::Checkmate(winner) => {
                if *winner == human_color {
                    ("win", "checkmate")
                } else {
                    ("loss", "checkmate")
                }
            }
            GameOutcome::Timeout(winner) => {
                if *winner == human_color {
                    ("win", "timeout")
                } else {
                    ("loss", "timeout")
                }
            }
            GameOutcome::Resignation(winner) => {
                if *winner == human_color {
                    ("win", "resignation")
                } else {
                    ("loss", "resignation")
                }
            }
            GameOutcome::Stalemate => ("draw", "stalemate"),
            GameOutcome::FiftyMoveRule => ("draw", "fifty-move rule"),
            GameOutcome::ThreefoldRepetition => ("draw", "threefold repetition"),
            GameOutcome::InsufficientMaterial => ("draw", "insufficient material"),
        };

        let tc = &local_game.time_control;
        let time_control_str = format!(
            "{}+{}",
            tc.initial_time_ms / 1000,
            tc.increment_ms / 1000
        );

        let engine_level = local_game.difficulty.label().to_string();
        let numeric_level = local_game.numeric_level;
        let was_adaptive = local_game.difficulty == LocalDifficulty::Adaptive;

        // Collect UCI moves from history
        let uci_moves: Vec<String> = state
            .local_history
            .iter()
            .filter_map(|snap| snap.move_uci.clone())
            .collect();

        let move_count = uci_moves.len() as i32;

        let (white_name, black_name) = if human_color == Color::White {
            (
                self.profile.as_ref().map_or("Player", |p| &p.name).to_string(),
                format!("Focalors ({})", engine_level),
            )
        } else {
            (
                format!("Focalors ({})", engine_level),
                self.profile.as_ref().map_or("Player", |p| &p.name).to_string(),
            )
        };

        let pgn_result = match result {
            "win" if human_color == Color::White => "1-0",
            "win" => "0-1",
            "loss" if human_color == Color::White => "0-1",
            "loss" => "1-0",
            _ => "1/2-1/2",
        };

        let today = chrono_today();
        let pgn = crate::db::generate_pgn(
            &uci_moves,
            &white_name,
            &black_name,
            pgn_result,
            Some(&time_control_str),
            Some(&today),
        );

        // Save to DB
        drop(state); // release lock before DB operations
        self.last_rating_change = None;
        if let Ok(game_id) = db.save_game(
            user_color,
            result,
            Some(result_reason),
            Some(&time_control_str),
            Some(&engine_level),
            &pgn,
            Some(move_count),
        ) {
            let _ = db.increment_games_played();

            // Detect and save opening name
            if let Some((_, opening_name)) = crate::openings::detect_opening(&uci_moves) {
                let _ = db.update_game_opening(game_id, opening_name);
            }

            // Update Elo rating
            let engine_elo = crate::strength::estimated_elo(numeric_level);
            if let Ok((old_rating, new_rating)) = db.update_rating_after_game(result, engine_elo) {
                let _ = db.update_game_rating(game_id, numeric_level, old_rating, new_rating);
                self.last_rating_change = Some((old_rating, new_rating));
                let delta = new_rating - old_rating;
                let sign = if delta >= 0 { "+" } else { "" };
                let mut s = self.state.lock().unwrap();
                s.status_message = format!(
                    "Game saved. Rating: {} -> {} ({}{}).",
                    old_rating, new_rating, sign, delta
                );
            }

            // Auto-adjust: if playing on Adaptive, check if level should change
            if was_adaptive {
                if let Ok(recent) =
                    db.get_recent_results(10, LocalDifficulty::Adaptive.label())
                {
                    if let Some(new_level) = crate::strength::evaluate_auto_adjust(self.adaptive_level, &recent) {
                        let old = self.adaptive_level;
                        self.adaptive_level = new_level;
                        let _ = db.set_adaptive_level(new_level);
                        let direction = if new_level > old { "up" } else { "down" };
                        self.auto_adjust_message = Some((
                            format!(
                                "Difficulty adjusted {} to Level {} (~{} Elo).",
                                direction,
                                new_level,
                                crate::strength::estimated_elo(new_level),
                            ),
                            std::time::Instant::now(),
                        ));
                    }
                }
            }

            // Refresh cached data
            self.profile = db.get_or_create_profile().ok();
            self.recent_games = db.get_recent_games(20).ok().unwrap_or_default();
            self.result_counts = db.get_result_counts().ok().unwrap_or((0, 0, 0));

            // Auto-show coaching report every 10 games
            if let Some(ref p) = self.profile {
                if p.games_played > 0 && p.games_played % 10 == 0 {
                    self.home_page = HomePage::Progress;
                }
            }
        }
    }

    // ── Analysis ────────────────────────────────────────────────────────

    pub(crate) fn start_analysis(&mut self, uci_moves: Vec<String>, user_color: Color) {
        let analysis_state = self.analysis_state.clone();

        // Generation stamp: starting a new analysis invalidates any
        // still-running older worker. Stale workers compare their stamp
        // against the counter before every state write and discard their
        // results — otherwise a slow analysis of game A finishing late
        // would overwrite game B's state and persist A's moves under
        // B's id (permanent DB corruption).
        let my_gen = self.analysis_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_counter = Arc::clone(&self.analysis_generation);
        let target_game_id = self.analysis_target_game_id;

        {
            let mut a = analysis_state.lock().unwrap();
            *a = AnalysisState::Running {
                progress: 0,
                total: uci_moves.len(),
            };
        }

        self.analysis_review_cursor = 0;

        let user_rating = self.profile.as_ref().map_or(1200, |p| p.rating);
        let analysis_depth = self.analysis_quality.depth();

        thread::spawn(move || {
            // RAII guard: if this worker exits via panic before the Complete
            // assignment below, AnalysisState would stay Running forever and
            // the UI spinner would hang. The guard runs on any unwind and
            // resets state to Idle so the user can retry. On the success path
            // the state has already moved to Complete, so the guard sees a
            // non-Running state and does nothing. A STALE worker's guard
            // must not touch state owned by a newer run, hence the
            // generation check.
            struct ResetGuard {
                state: Arc<Mutex<AnalysisState>>,
                gen_counter: Arc<AtomicU64>,
                my_gen: u64,
            }
            impl Drop for ResetGuard {
                fn drop(&mut self) {
                    if self.gen_counter.load(Ordering::SeqCst) != self.my_gen {
                        return;
                    }
                    let mut a = match self.state.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if matches!(*a, AnalysisState::Running { .. }) {
                        *a = AnalysisState::Idle;
                        eprintln!(
                            "focalors: analysis worker exited unexpectedly; \
                             UI reset to Idle"
                        );
                    }
                }
            }
            let _reset_guard = ResetGuard {
                state: analysis_state.clone(),
                gen_counter: gen_counter.clone(),
                my_gen,
            };

            crate::attacks::init();
            let use_nnue = crate::nnue::network::get_network().is_some();
            let result = crate::analysis::analyze_game(
                &uci_moves,
                user_color,
                analysis_depth,
                use_nnue,
                &mut |cur, tot| {
                    if gen_counter.load(Ordering::SeqCst) != my_gen {
                        return; // superseded — don't clobber the new run's progress
                    }
                    let mut a = analysis_state.lock().unwrap();
                    *a = AnalysisState::Running {
                        progress: cur,
                        total: tot,
                    };
                },
            );
            // Extract puzzles from blunders
            let puzzles = crate::puzzles::extract_puzzles(
                &uci_moves, &result, user_color, user_rating,
            );
            let mut a = analysis_state.lock().unwrap();
            if gen_counter.load(Ordering::SeqCst) != my_gen {
                return; // superseded while finishing — discard silently
            }
            *a = AnalysisState::Complete {
                analysis: result,
                puzzles,
                uci_moves,
                game_id: target_game_id,
            };
        });
    }

    /// Hand a finished live game over to Game Review in one step: leave
    /// the live-play screen, open the just-saved game in the replay panel
    /// and start the analysis worker so progress and the result render on
    /// the Analyze page. The game is already in the DB by the time the
    /// outcome banner shows (auto-save runs before any drawing), so the
    /// most recent saved game is this one.
    pub(crate) fn review_finished_game(&mut self) {
        let (uci_moves, user_color) = {
            let state = self.state.lock().unwrap();
            let uci_moves: Vec<String> = state
                .local_history
                .iter()
                .filter_map(|s| s.move_uci.clone())
                .collect();
            (uci_moves, state.local_game.human_color)
        };
        if uci_moves.is_empty() {
            return;
        }
        let game_id = self
            .db
            .as_ref()
            .and_then(|db| db.get_recent_games(1).ok())
            .and_then(|g| g.into_iter().next())
            .map(|g| g.id);

        self.abort_local_game();

        if let Some(id) = game_id {
            self.start_replay(id);
            self.analysis_target_game_id = Some(id);
        }
        // A fresh game never has saved analysis, but start_replay hydrates
        // one when it exists; only spawn the worker when it did not.
        let already_analyzed = matches!(
            *self.analysis_state.lock().unwrap(),
            AnalysisState::Complete { game_id: Some(id), .. } if Some(id) == game_id
        );
        if !already_analyzed {
            self.start_analysis(uci_moves, user_color);
        }
        self.home_page = HomePage::Analyze;
        self.state.lock().unwrap().status_message =
            "Game saved. Reviewing your last game.".to_string();
    }

    pub(crate) fn draw_analysis_progress(&self, ui: &mut egui::Ui) {
        let analysis = self.analysis_state.lock().unwrap();
        if let AnalysisState::Running { progress, total } = *analysis {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(hydra_heading(
                    format!(
                        "Analyzing {progress}/{total} moves · {}",
                        self.analysis_quality.label()
                    ),
                    13.0,
                ));
            });
            ui.add_space(6.0);
            if total > 0 {
                let frac = progress as f32 / total as f32;
                ui.add(egui::ProgressBar::new(frac).fill(hydra_accent()));
            }
        }
    }

    /// One-shot persistence of completed analysis. Persists puzzles + move
    /// analysis against the chosen game id (preferring `analysis_target_game_id`
    /// when set, falling back to the most recent saved game). Clears `puzzles`
    /// and `uci_moves` on the `Complete` state to mark "already saved".
    pub(crate) fn persist_completed_analysis(&mut self) {
        let acc_opt: Option<f64> = {
            let analysis = self.analysis_state.lock().unwrap();
            if let AnalysisState::Complete { puzzles, analysis: ga, uci_moves, game_id } =
                &*analysis
            {
                if let Some(ref db) = self.db {
                    if !puzzles.is_empty() {
                        for p in puzzles {
                            let _ = db.save_puzzle(
                                p.game_id,
                                &p.fen,
                                &p.solution_uci,
                                p.theme.to_db_str(),
                                p.rating,
                            );
                        }
                    }
                    if !uci_moves.is_empty() {
                        // Use the id captured when the worker was spawned —
                        // NOT current UI state, which may point at a
                        // different game by the time a slow analysis lands.
                        let game_id = game_id.or_else(|| {
                            db.get_recent_games(1)
                                .ok()
                                .and_then(|g| g.into_iter().next())
                                .map(|g| g.id)
                        });
                        if let Some(gid) = game_id {
                            let _ = db.save_move_analysis(gid, uci_moves, &ga.moves);
                            // No accuracy (all moves book/forced) stays NULL:
                            // such games never enter the Progress aggregates,
                            // which filter on user_accuracy IS NOT NULL.
                            if let Some(acc) = ga.user_accuracy {
                                let _ = db.update_game_accuracy(gid, acc);
                            }
                        }
                    }
                }
                ga.user_accuracy
            } else {
                None
            }
        };
        if let Some(acc) = acc_opt {
            if self.session_best_accuracy.map_or(true, |best| acc > best) {
                self.session_best_accuracy = Some(acc);
            }
        }
        // Clear puzzles + uci_moves to mark "saved" so we don't re-save next frame.
        let mut analysis = self.analysis_state.lock().unwrap();
        if let AnalysisState::Complete { puzzles, uci_moves, .. } = &mut *analysis {
            puzzles.clear();
            uci_moves.clear();
        }
    }

    /// Dedicated full-page game-review surface. Composes the board, eval
    /// graph, classified move list, per-move detail, and navigation in a
    /// proper two-column layout instead of the cramped embedded card.
    /// Empty state when no game is loaded — user is told to pick from
    /// History or paste a PGN in Import.
    pub(crate) fn draw_analyze_page(&mut self, ui: &mut egui::Ui) {
        // Make sure a freshly-completed analysis gets persisted before we read it.
        self.persist_completed_analysis();

        // ── Empty state — no game loaded ────────────────────────────────
        if self.replay_game.is_none() {
            ui.horizontal(|ui| {
                ui.label(hydra_heading("Analyze a Game", 22.0));
                ui.label(
                    egui::RichText::new("Pick a recent game or paste a PGN")
                        .color(hydra_subtle_text()),
                );
            });
            ui.add_space(16.0);

            let mut replay_id: Option<i64> = None;
            let mut imported: Option<crate::pgn::ParsedPgn> = None;
            let mut clear_pgn = false;
            let mut goto_history = false;
            let games_snapshot: Vec<crate::db::SavedGame> = self.recent_games.clone();

            // Live PGN parse — drives the validation badge and the parsed
            // panel state without requiring an explicit "Parse" click.
            // Re-parsed only when the text actually changes.
            let pgn_validation = if self.pgn_import_text.trim().is_empty() {
                self.pgn_parse_cache = None;
                None
            } else {
                let stale = self
                    .pgn_parse_cache
                    .as_ref()
                    .is_none_or(|(text, _)| *text != self.pgn_import_text);
                if stale {
                    let result = crate::pgn::parse_pgn(&self.pgn_import_text);
                    self.pgn_parse_cache = Some((self.pgn_import_text.clone(), result));
                }
                self.pgn_parse_cache.as_ref().map(|(_, r)| r.clone())
            };
            // Resync the suggested user color whenever the parsed PGN
            // changes — first call after a paste, then leave it sticky so
            // the user can override their side and the selection survives.
            if let Some(Ok(ref parsed)) = pgn_validation {
                if self.pgn_import_parsed.as_ref().map(|p| &p.uci_moves) != Some(&parsed.uci_moves) {
                    let profile_name = self.profile.as_ref().map(|p| p.name.clone());
                    self.pgn_import_user_color = crate::pgn::user_color_from_headers(
                        parsed,
                        profile_name.as_deref(),
                    );
                    self.pgn_import_parsed = Some(parsed.clone());
                }
            } else {
                self.pgn_import_parsed = None;
            }

            ui.spacing_mut().item_spacing.x = CARD_GAP;
            ui.columns(2, |cols| {
                // ── LEFT: recent games list ────────────────────────────
                hydra_card_frame().show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ANALYZE_ROW_H);
                    ui.horizontal(|ui| {
                        ui.label(hydra_heading("Your Recent Games", 14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Open History").clicked() {
                                goto_history = true;
                            }
                        });
                    });
                    ui.add_space(8.0);

                    if games_snapshot.is_empty() {
                        ui.add_space(80.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("No games yet.")
                                    .size(13.0).color(hydra_subtle_text()),
                            );
                            ui.label(
                                egui::RichText::new("Play a game and it'll show up here.")
                                    .size(11.0).color(hydra_subtle_text()),
                            );
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("analyze_recent")
                            .max_height(ANALYZE_ROW_H - 96.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                            for game in &games_snapshot {
                                let result_color = match game.result.as_str() {
                                    "win" => hydra_success(),
                                    "loss" => hydra_danger(),
                                    _ => class_inaccuracy(),
                                };
                                let result_label = match game.result.as_str() {
                                    "win" => "W",
                                    "loss" => "L",
                                    "draw" => "D",
                                    _ => "?",
                                };
                                let date = &game.played_at[..10.min(game.played_at.len())];
                                let moves = game.move_count.unwrap_or(0);
                                let tc = game.time_control.as_deref().unwrap_or("");
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(" {result_label} "))
                                            .color(hydra_text_on_accent())
                                            .background_color(result_color)
                                            .strong()
                                            .monospace(),
                                    );
                                    ui.label(
                                        egui::RichText::new(date).size(11.0).strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "as {} · {moves} moves",
                                            game.user_color
                                        ))
                                        .size(10.0).color(hydra_subtle_text()),
                                    );
                                    if !tc.is_empty() {
                                        ui.label(
                                            egui::RichText::new(format!("· {tc}"))
                                                .size(10.0).color(hydra_subtle_text()),
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Review").clicked() {
                                                replay_id = Some(game.id);
                                            }
                                        },
                                    );
                                });
                                ui.add_space(4.0);
                                ui.separator();
                            }
                        });
                    }
                });

                // ── RIGHT: PGN paste with live validation ──────────────
                hydra_card_frame().show(&mut cols[1], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ANALYZE_ROW_H);
                    ui.label(hydra_heading("Import PGN", 14.0));
                    ui.label(
                        egui::RichText::new("Paste a game's PGN to run a full engine review.")
                            .size(11.0).color(hydra_subtle_text()),
                    );
                    ui.add_space(8.0);

                    ui.add(
                        egui::TextEdit::multiline(&mut self.pgn_import_text)
                            .font(egui::TextStyle::Monospace)
                            .desired_rows(18)
                            .desired_width(f32::INFINITY)
                            .hint_text(
                                "[Event \"…\"]\n[White \"…\"]\n…\n\n1. e4 e5 2. Nf3 Nc6 …",
                            ),
                    );
                    ui.add_space(8.0);

                    match &pgn_validation {
                        Some(Ok(parsed)) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new("✓ Valid")
                                        .size(12.0).strong().color(hydra_success()),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "· {} moves · {} vs {}",
                                        parsed.uci_moves.len(),
                                        parsed.white.as_deref().unwrap_or("?"),
                                        parsed.black.as_deref().unwrap_or("?"),
                                    ))
                                    .size(11.0).color(hydra_subtle_text()),
                                );
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Analyze as")
                                        .size(11.0).color(hydra_subtle_text()).strong(),
                                );
                                if ui
                                    .selectable_label(
                                        self.pgn_import_user_color == Color::White,
                                        "White",
                                    )
                                    .clicked()
                                {
                                    self.pgn_import_user_color = Color::White;
                                }
                                if ui
                                    .selectable_label(
                                        self.pgn_import_user_color == Color::Black,
                                        "Black",
                                    )
                                    .clicked()
                                {
                                    self.pgn_import_user_color = Color::Black;
                                }
                            });
                            ui.add_space(10.0);
                            if ui
                                .add_sized(
                                    [ui.available_width(), 40.0],
                                    primary_button("Save & Review"),
                                )
                                .clicked()
                            {
                                imported = Some(parsed.clone());
                            }
                        }
                        Some(Err(err)) => {
                            egui::Frame::new()
                                .fill(hydra_danger().gamma_multiply(0.10))
                                .stroke(egui::Stroke::new(1.0_f32, hydra_danger()))
                                .corner_radius(6)
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            egui::RichText::new("✗")
                                                .size(13.0).color(hydra_danger()).strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(err)
                                                .size(11.0).color(hydra_danger()),
                                        );
                                    });
                                });
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("Paste a PGN above to validate.")
                                    .size(11.0).color(hydra_subtle_text()),
                            );
                        }
                    }

                    if !self.pgn_import_text.is_empty() {
                        ui.add_space(6.0);
                        if ui.small_button("Clear").clicked() {
                            clear_pgn = true;
                        }
                    }
                });
            });

            // Apply deferred actions outside the columns closure so we
            // don't fight with the mutable borrow of self inside it.
            if let Some(id) = replay_id {
                self.start_replay(id);
            }
            if let Some(parsed) = imported {
                self.save_imported_pgn_and_open_review(&parsed);
            }
            if clear_pgn {
                self.pgn_import_text.clear();
                self.pgn_import_parsed = None;
                self.pgn_import_error = None;
            }
            if goto_history {
                self.home_page = HomePage::History;
            }
            return;
        }

        // ── Snapshot state for this frame ──────────────────────────────
        let (cursor, num_plies, board, last_move, game_id, header_text, user_color, uci_moves) = {
            let r = self.replay_game.as_ref().unwrap();
            let total_plies = r.moves.len();
            let cursor = r.cursor.min(total_plies);
            let board = r.boards[cursor].clone();
            let last_move = if cursor > 0 {
                r.moves.get(cursor - 1).copied()
            } else {
                None
            };
            let result_word = match r.game.result.as_str() {
                "win" => "Win",
                "loss" => "Loss",
                "draw" => "Draw",
                other => other,
            };
            let date = r.game.played_at.get(..10).unwrap_or("");
            let header = match r.game.result_reason.as_deref().filter(|s| !s.is_empty()) {
                Some(reason) => format!("{result_word} by {reason} · {date}"),
                None => format!("{result_word} · {date}"),
            };
            let user_color = if r.game.user_color == "white" {
                Color::White
            } else {
                Color::Black
            };
            (
                cursor,
                total_plies,
                board,
                last_move,
                r.game.id,
                header,
                user_color,
                r.uci_moves.clone(),
            )
        };

        let analysis_for_this_game: Option<crate::analysis::GameAnalysis> = {
            let a = self.analysis_state.lock().unwrap();
            match &*a {
                AnalysisState::Complete { analysis, .. }
                    if self.analysis_target_game_id == Some(game_id) =>
                {
                    Some(analysis.clone())
                }
                _ => None,
            }
        };

        // Game Review overlay arrows. Tied to `cursor` (the board currently on
        // screen), not `new_cursor` (whose board lands a frame later), so the
        // arrows always match the pieces shown. Describes the move that produced
        // this position: analysis.moves[cursor - 1].
        let review_overlay: Option<ReviewOverlay> = if cursor > 0 {
            analysis_for_this_game
                .as_ref()
                .and_then(|ga| ga.moves.get(cursor - 1))
                .map(|ma| ReviewOverlay {
                    best: uci_arrow_squares(&ma.best_move_uci),
                    played: last_move
                        .filter(|m| !m.is_null())
                        .map(|m| (m.from_sq().0, m.to_sq().0)),
                    played_color: classification_color(ma.classification),
                    played_class: ma.classification,
                })
        } else {
            None
        };

        // Keyboard navigation
        let (key_left, key_right, key_home, key_end) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
            )
        });
        let mut new_cursor = cursor;
        if key_left && new_cursor > 0 {
            new_cursor -= 1;
        }
        if key_right && new_cursor < num_plies {
            new_cursor += 1;
        }
        if key_home {
            new_cursor = 0;
        }
        if key_end {
            new_cursor = num_plies;
        }

        let mut want_close = false;
        let mut want_analyze = false;
        let mut pick_quality: Option<AnalysisQuality> = None;

        // ── Header strip ────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(hydra_heading("Game Review", 22.0));
            ui.label(egui::RichText::new(header_text).color(hydra_subtle_text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([88.0, 28.0], secondary_button("Close")).clicked() {
                    want_close = true;
                }
            });
        });

        if num_plies == 0 {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Could not parse this game's PGN, so there is nothing to review.")
                    .color(hydra_warning()),
            );
            if want_close {
                self.replay_game = None;
            }
            return;
        }

        ui.add_space(SECTION_GAP);

        // ── Two-column body, centered horizontally ─────────────────────
        ui.horizontal_top(|ui| {
            // Center the board+panel block on wide screens by padding the
            // start of the horizontal layout.
            let gap_w: f32 = CARD_GAP;
            let right_w: f32 = 420.0;
            let avail = ui.available_width();
            // The board column gives way on narrow windows instead of pushing
            // the right panel off-screen: draw_board sizes itself to whatever
            // width it gets, so shrinking here is safe. At the 800px minimum
            // window the board lands near 320px; on wide screens it stays 760.
            let board_w: f32 = (avail - gap_w - right_w).clamp(320.0, 760.0);
            let total = board_w + gap_w + right_w;
            if avail > total {
                ui.add_space((avail - total) / 2.0);
            }

            // Left: big read-only board with the step controls under it.
            ui.vertical(|ui| {
                ui.set_max_width(board_w);
                let view = BoardView {
                    board: &board,
                    last_move,
                    interactive: false,
                    review: review_overlay,
                };
                // Orient the review board to the reviewed player, but only for
                // this draw: save and restore the shared `self.flipped` so that
                // reviewing never re-orients the live-play / puzzle board, which
                // keep using it. Without this, reviewing an opposite-color game
                // would leave the live board flipped afterwards.
                let saved_flip = self.flipped;
                self.flipped = user_color == Color::Black;
                self.draw_board(ui, Some(&view));
                self.flipped = saved_flip;
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([36.0, 28.0], secondary_button("⏮")).clicked() {
                        new_cursor = 0;
                    }
                    if ui.add_sized([84.0, 28.0], secondary_button("◀ Prev")).clicked()
                        && new_cursor > 0
                    {
                        new_cursor -= 1;
                    }
                    ui.label(
                        egui::RichText::new(format!("Move {} / {}", new_cursor, num_plies))
                            .size(12.0)
                            .color(hydra_subtle_text()),
                    );
                    if ui.add_sized([84.0, 28.0], secondary_button("Next ▶")).clicked()
                        && new_cursor < num_plies
                    {
                        new_cursor += 1;
                    }
                    if ui.add_sized([36.0, 28.0], secondary_button("⏭")).clicked() {
                        new_cursor = num_plies;
                    }
                });
            });

            ui.add_space(gap_w);

            // Right: one panel card as tall as the board, the same shape as
            // the puzzle trainer. Summary and eval graph on top, the move
            // list in the middle, the coach note for the selected move at
            // the bottom. The card's minimum height matches the board (plus
            // its coordinate strip) so the two columns end on one line.
            ui.vertical(|ui| {
                ui.set_max_width(right_w);
                let card_h = board_w + 28.0;
                // Everything except the move list is roughly 350px tall;
                // the list gets whatever is left under the board's edge.
                let list_h = (card_h - 36.0 - 350.0).clamp(120.0, 400.0);
                let running = matches!(
                    *self.analysis_state.lock().unwrap(),
                    AnalysisState::Running { .. }
                );
                hydra_card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(card_h - 36.0);

                    if let Some(ref ga) = analysis_for_this_game {
                        // Accuracy next to the classification counts.
                        let mut counts = [0u32; 6];
                        for m in &ga.moves {
                            if m.side != ga.user_color {
                                continue;
                            }
                            match m.classification {
                                crate::analysis::MoveClass::Best => counts[0] += 1,
                                crate::analysis::MoveClass::Good => counts[1] += 1,
                                crate::analysis::MoveClass::Book => counts[2] += 1,
                                crate::analysis::MoveClass::Inaccuracy => counts[3] += 1,
                                crate::analysis::MoveClass::Mistake => counts[4] += 1,
                                crate::analysis::MoveClass::Blunder => counts[5] += 1,
                                _ => {}
                            }
                        }
                        let classes = [
                            ("Best", crate::analysis::MoveClass::Best),
                            ("Good", crate::analysis::MoveClass::Good),
                            ("Book", crate::analysis::MoveClass::Book),
                            ("Inaccuracy", crate::analysis::MoveClass::Inaccuracy),
                            ("Mistake", crate::analysis::MoveClass::Mistake),
                            ("Blunder", crate::analysis::MoveClass::Blunder),
                        ];
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(104.0);
                                ui.label(hydra_heading("ACCURACY", 10.0).color(hydra_subtle_text()));
                                ui.add_space(2.0);
                                match ga.user_accuracy {
                                    Some(acc) => {
                                        ui.label(
                                            hydra_heading(format!("{acc:.1}%"), 22.0)
                                                .color(accuracy_color(acc)),
                                        );
                                        ui.label(
                                            egui::RichText::new("your moves")
                                                .size(11.0)
                                                .color(hydra_subtle_text()),
                                        );
                                    }
                                    None => {
                                        // Nothing gradable: every user move was
                                        // book or forced.
                                        ui.label(hydra_heading("n/a", 22.0).color(hydra_subtle_text()));
                                        ui.label(
                                            egui::RichText::new("all theory")
                                                .size(11.0)
                                                .color(hydra_subtle_text()),
                                        );
                                    }
                                }
                            });
                            ui.vertical(|ui| {
                                ui.label(hydra_heading("MOVE QUALITY", 10.0).color(hydra_subtle_text()));
                                ui.add_space(2.0);
                                egui::Grid::new("review_class_counts")
                                    .num_columns(3)
                                    .spacing([14.0, 2.0])
                                    .show(ui, |ui| {
                                        for (i, (label, class)) in classes.iter().enumerate() {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    hydra_heading(counts[i].to_string(), 14.0)
                                                        .color(classification_color(*class)),
                                                );
                                                ui.label(
                                                    egui::RichText::new(*label)
                                                        .size(11.0)
                                                        .color(hydra_subtle_text()),
                                                );
                                            });
                                            if i % 3 == 2 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                        });

                        ui.add_space(SECTION_GAP);
                        ui.label(hydra_heading("EVALUATION", 10.0).color(hydra_subtle_text()));
                        ui.add_space(4.0);
                        let points: Vec<[f64; 2]> = ga
                            .eval_history
                            .iter()
                            .enumerate()
                            .map(|(i, &e)| [i as f64, (e as f64 / 100.0).clamp(-5.0, 5.0)])
                            .collect();
                        let line = egui_plot::Line::new("eval", egui_plot::PlotPoints::new(points))
                            .color(hydra_accent())
                            .width(2.0_f32)
                            .fill(0.0_f32)
                            .fill_alpha(0.14_f32);
                        let zero = egui_plot::HLine::new("zero", 0.0)
                            .color(hydra_border())
                            .width(1.0_f32);
                        egui_plot::Plot::new("analyze_eval_graph")
                            .height(110.0)
                            .include_y(-3.0)
                            .include_y(3.0)
                            .allow_drag(false)
                            .allow_zoom(false)
                            .allow_scroll(false)
                            .show_x(false)
                            .show_y(false)
                            .show_background(false)
                            .show_grid(false)
                            .show_axes([false, true])
                            .show(ui, |plot_ui| {
                                plot_ui.hline(zero);
                                plot_ui.line(line);
                                if new_cursor > 0 {
                                    let vline = egui_plot::VLine::new(
                                        "cursor",
                                        new_cursor as f64,
                                    )
                                    .color(hydra_warning())
                                    .width(1.5_f32);
                                    plot_ui.vline(vline);
                                }
                            });
                    } else if running {
                        self.draw_analysis_progress(ui);
                    } else {
                        ui.label(hydra_heading("Not analyzed yet", 14.0));
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    "Run the analysis to grade every move and get a coach note on each one.",
                                )
                                .size(12.0)
                                .color(hydra_subtle_text()),
                            )
                            .wrap(),
                        );
                        if !uci_moves.is_empty() {
                            ui.add_space(10.0);
                            ui.label(hydra_heading("DEPTH", 10.0).color(hydra_subtle_text()));
                            ui.horizontal(|ui| {
                                for quality in AnalysisQuality::ALL {
                                    let selected = self.analysis_quality == quality;
                                    if ui.add(home_nav_button(selected, quality.label())).clicked()
                                        && !selected
                                    {
                                        pick_quality = Some(quality);
                                    }
                                }
                            });
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(self.analysis_quality.hint())
                                        .size(11.0)
                                        .color(hydra_subtle_text()),
                                )
                                .wrap(),
                            );
                            ui.add_space(10.0);
                            if ui
                                .add_sized([ui.available_width(), 34.0], primary_button("Run Analysis"))
                                .clicked()
                            {
                                want_analyze = true;
                            }
                        }
                    }

                    ui.add_space(SECTION_GAP);
                    ui.label(hydra_heading("MOVES", 10.0).color(hydra_subtle_text()));
                    ui.add_space(4.0);

                    // Grid-based move list: one row per full move, three columns
                    // (number, white move, black move). Each move is a clickable
                    // selectable label, color-coded by classification.
                    let n_pairs = (num_plies + 1) / 2;
                    let render_ply = |ui: &mut egui::Ui, ply: usize, new_cursor: &mut usize| {
                        if ply >= num_plies {
                            ui.label("");
                            return;
                        }
                        let san_or_uci = analysis_for_this_game
                            .as_ref()
                            .and_then(|ga| ga.moves.get(ply))
                            .map(|m| m.move_san.clone())
                            .unwrap_or_else(|| uci_moves.get(ply).cloned().unwrap_or_default());
                        let class_color = analysis_for_this_game
                            .as_ref()
                            .and_then(|ga| ga.moves.get(ply))
                            .map(|m| classification_color(m.classification))
                            .unwrap_or_else(hydra_text);
                        let symbol = analysis_for_this_game
                            .as_ref()
                            .and_then(|ga| ga.moves.get(ply))
                            .map(|m| m.classification.symbol())
                            .unwrap_or("");
                        let label = format!("{san_or_uci}{symbol}");
                        let selected = *new_cursor == ply + 1;
                        // The selected move sits on the accent fill (the
                        // theme's selection color in both modes), where the
                        // classification color is unreadable; it takes the
                        // text-on-accent white like a primary button, and
                        // the note below still names the class.
                        let text_color = if selected { hydra_text_on_accent() } else { class_color };
                        let resp = ui.selectable_label(
                            selected,
                            egui::RichText::new(label)
                                .size(12.5)
                                .strong()
                                .color(text_color),
                        );
                        if resp.clicked() {
                            *new_cursor = ply + 1;
                        }
                    };

                    egui::ScrollArea::vertical()
                        .id_salt("analyze_move_list")
                        .auto_shrink([false, true])
                        .max_height(list_h)
                        .show(ui, |ui| {
                            egui::Grid::new("analyze_moves_grid")
                                .num_columns(3)
                                .spacing([10.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    for pair_idx in 0..n_pairs {
                                        ui.label(
                                            egui::RichText::new(format!("{}.", pair_idx + 1))
                                                .size(11.0)
                                                .color(hydra_subtle_text()),
                                        );
                                        render_ply(ui, pair_idx * 2, &mut new_cursor);
                                        render_ply(ui, pair_idx * 2 + 1, &mut new_cursor);
                                        ui.end_row();
                                    }
                                });
                        });

                    // Coach note for the selected move, under the list so the
                    // list never jumps as the note's length changes. When the
                    // mascot lands (first full release) it sits to the left of
                    // this block, which then becomes its speech bubble.
                    if let Some(ga) = analysis_for_this_game.as_ref() {
                        ui.add_space(SECTION_GAP);
                        if new_cursor > 0
                            && let Some(ma) = ga.moves.get(new_cursor - 1)
                        {
                            // Best move as SAN needs the position BEFORE the move
                            // (boards[new_cursor - 1]); fall back to raw UCI if that
                            // board or the parse is unavailable.
                            let best_san = self
                                .replay_game
                                .as_ref()
                                .and_then(|r| r.boards.get(new_cursor - 1))
                                .map(|b| crate::db::uci_to_san(b, &ma.best_move_uci))
                                .unwrap_or_else(|| ma.best_move_uci.clone());
                            let coach = crate::analysis::coach_line(ma, &best_san);
                            ui.label(
                                hydra_heading(ma.classification.label(), 13.0)
                                    .color(classification_color(ma.classification)),
                            );
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&coach)
                                        .size(12.0)
                                        .color(hydra_text()),
                                )
                                .wrap(),
                            );
                            // Rich in-session explanation (hanging pieces, the
                            // engine line). Not persisted, so only fresh
                            // analyses have it; the coach line above always
                            // shows either way.
                            if let Some(ref expl) = ma.explanation {
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(expl)
                                            .size(10.5)
                                            .color(hydra_subtle_text()),
                                    )
                                    .wrap(),
                                );
                            }
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "CPL {} · Eval {} to {} · Best {} ({})",
                                        ma.cpl,
                                        format_eval(ma.eval_before),
                                        format_eval(ma.eval_after),
                                        best_san,
                                        format_eval(ma.best_eval),
                                    ))
                                    .size(10.5)
                                    .color(hydra_subtle_text()),
                                )
                                .wrap(),
                            );
                        } else {
                            ui.label(hydra_heading("COACH", 10.0).color(hydra_subtle_text()));
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new("Step through the moves to see a note on each one.")
                                    .size(12.0)
                                    .color(hydra_subtle_text()),
                            );
                        }
                    }
                });
            });
        });

        // Apply state mutations after the closures
        let mut review_sfx: Option<crate::sound::Sfx> = None;
        if new_cursor != cursor {
            if let Some(r) = self.replay_game.as_mut() {
                let target = new_cursor.min(r.moves.len());
                // The move that produced the target position, whichever
                // direction we're stepping — scrubbing to a position plays
                // that position's move/capture sound, same as the live board.
                if target > 0 {
                    let mv = r.moves[target - 1];
                    let captured = is_capture_move(&r.boards[target - 1], mv);
                    review_sfx = Some(move_sfx(captured));
                }
                r.cursor = target;
            }
        }
        if let Some(sfx) = review_sfx {
            self.play_sound(sfx);
        }
        if let Some(quality) = pick_quality {
            self.set_analysis_quality(quality);
        }
        if want_close {
            self.replay_game = None;
        }
        if want_analyze {
            let user_color = if self.replay_game.as_ref().map(|r| r.game.user_color.as_str())
                == Some("white")
            {
                Color::White
            } else {
                Color::Black
            };
            self.analysis_target_game_id = Some(game_id);
            self.start_analysis(uci_moves, user_color);
        }
    }

    /// Persist a parsed PGN as a `SavedGame` with `engine_level = "imported"`,
    /// then route the user into the History → replay panel for that game.
    /// Imports get the same review pipeline as locally played games.
    pub(crate) fn save_imported_pgn_and_open_review(&mut self, parsed: &crate::pgn::ParsedPgn) {
        let db = match self.db.as_ref() {
            Some(d) => d,
            None => return,
        };
        let user_color_str = if self.pgn_import_user_color == Color::White {
            "white"
        } else {
            "black"
        };
        let result_token = parsed.result.as_deref().unwrap_or("*");
        let result = match (result_token, self.pgn_import_user_color) {
            ("1-0", Color::White) | ("0-1", Color::Black) => "win",
            ("1-0", Color::Black) | ("0-1", Color::White) => "loss",
            ("1/2-1/2", _) => "draw",
            _ => "draw",
        };
        // Re-emit a clean PGN from the UCI moves so generated SAN matches what
        // the replay panel expects.
        let white = parsed.white.clone().unwrap_or_else(|| "?".to_string());
        let black = parsed.black.clone().unwrap_or_else(|| "?".to_string());
        let pgn = crate::db::generate_pgn(
            &parsed.uci_moves,
            &white,
            &black,
            result_token,
            None,
            Some(&chrono_today()),
        );
        let move_count = parsed.uci_moves.len() as i32;

        let game_id = match db.save_game(
            user_color_str,
            result,
            Some("imported"),
            None,
            Some("imported"),
            &pgn,
            Some(move_count),
        ) {
            Ok(id) => id,
            Err(_) => return,
        };

        if let Some((_, opening_name)) = crate::openings::detect_opening(&parsed.uci_moves) {
            let _ = db.update_game_opening(game_id, opening_name);
        }

        self.recent_games = db.get_recent_games(20).ok().unwrap_or_default();
        self.pgn_import_text.clear();
        self.pgn_import_parsed = None;
        self.pgn_import_error = None;

        self.start_replay(game_id);
        self.home_page = HomePage::Analyze;
    }

    // ── Settings window ────────────────────────────────────────────────
}
