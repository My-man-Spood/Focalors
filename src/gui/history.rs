use super::*;

impl FocalorsApp {
    pub(crate) fn draw_game_history(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(hydra_heading("Game History", 22.0));
            ui.label(
                egui::RichText::new(format!("{} games", self.recent_games.len()))
                    .color(hydra_subtle_text()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Back to Overview").clicked() {
                    self.home_page = HomePage::Overview;
                }
            });
        });
        ui.add_space(SECTION_GAP);

        if self.recent_games.is_empty() {
            hydra_card_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    egui::RichText::new("No games yet. Play your first game!")
                        .size(12.0)
                        .color(hydra_subtle_text()),
                );
            });
            return;
        }

        // Snapshot so the thumbnail cache can borrow self mutably in the loop.
        let games_snapshot: Vec<crate::db::SavedGame> = self.recent_games.clone();
        let mut replay_id = None;

        // As many card columns as fit, one to three.
        let cols_n = ((ui.available_width() + CARD_GAP) / (HISTORY_TILE_MIN_W + CARD_GAP))
            .floor()
            .clamp(1.0, 3.0) as usize;

        egui::ScrollArea::vertical()
            .id_salt("history_grid")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(CARD_GAP, CARD_GAP);
                for row in games_snapshot.chunks(cols_n) {
                    ui.columns(cols_n, |cols| {
                        for (i, game) in row.iter().enumerate() {
                            let result_icon = match game.result.as_str() {
                                "win" => "W",
                                "loss" => "L",
                                "draw" => "D",
                                _ => "?",
                            };
                            let result_color = match game.result.as_str() {
                                "win" => hydra_success(),
                                "loss" => hydra_danger(),
                                _ => class_inaccuracy(),
                            };
                            let reason = game.result_reason.as_deref().unwrap_or("");
                            let tc = game.time_control.as_deref().unwrap_or("");
                            let level = game.engine_level.as_deref().unwrap_or("");
                            let moves = game.move_count.unwrap_or(0);
                            let date = &game.played_at[..10.min(game.played_at.len())];

                            hydra_card_frame().show(&mut cols[i], |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let thumb = self.thumbnail_for_game(game);
                                    if let Some(ref board) = thumb {
                                        draw_board_thumbnail(ui, board, 120.0);
                                    } else {
                                        ui.allocate_space(egui::vec2(120.0, 120.0));
                                    }
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(" {result_icon} "))
                                                    .color(hydra_text_on_accent())
                                                    .background_color(result_color)
                                                    .strong()
                                                    .monospace(),
                                            );
                                            ui.label(hydra_heading(
                                                format!("{} as {}", date, game.user_color),
                                                12.0,
                                            ));
                                        });
                                        if !reason.is_empty() {
                                            ui.label(
                                                egui::RichText::new(reason)
                                                    .size(11.0)
                                                    .color(hydra_subtle_text()),
                                            );
                                        }
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}{} ({} moves)",
                                                if tc.is_empty() { "" } else { tc },
                                                if level.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(" vs {level}")
                                                },
                                                moves,
                                            ))
                                            .size(11.0)
                                            .color(hydra_subtle_text()),
                                        );
                                        ui.add_space(6.0);
                                        if ui.small_button("Open in Analyze").clicked() {
                                            replay_id = Some(game.id);
                                        }
                                    });
                                });
                            });
                        }
                    });
                }
            });

        if let Some(id) = replay_id {
            self.start_replay(id);
            self.home_page = HomePage::Analyze;
        }
    }

    /// Get (or compute and cache) the final-position board for a game,
    /// used to render History thumbnails. Returns None if the game's PGN
    /// can't be parsed.
    pub(crate) fn thumbnail_for_game(&mut self, game: &crate::db::SavedGame) -> Option<Board> {
        if !self.history_thumbnails.contains_key(&game.id) {
            let board = build_replay_state(game.clone())
                .boards
                .last()
                .cloned();
            self.history_thumbnails.insert(game.id, board);
        }
        self.history_thumbnails.get(&game.id).cloned().flatten()
    }

    pub(crate) fn start_replay(&mut self, game_id: i64) {
        let game = match self.db.as_ref().and_then(|db| db.get_game(game_id).ok()) {
            Some(g) => g,
            None => return,
        };

        // If this game has saved analysis, hydrate the in-memory state so
        // the replay panel shows it immediately instead of pretending the
        // game has never been analyzed.
        if let Some(ref db) = self.db {
            // Saved move rows are the "was analyzed" marker; the accuracy may
            // legitimately be NULL (a game whose gradable moves were all
            // book/forced stores no accuracy).
            let saved_accuracy = db.get_game_accuracy(game_id).ok().flatten();
            if let Ok(moves) = db.get_move_analysis(game_id) {
                if !moves.is_empty() {
                    let user_color = if game.user_color == "white" {
                        Color::White
                    } else {
                        Color::Black
                    };
                    let mut eval_history = Vec::with_capacity(moves.len() + 1);
                    eval_history.push(moves[0].eval_before);
                    for m in &moves {
                        eval_history.push(m.eval_after);
                    }
                    let analysis = crate::analysis::GameAnalysis {
                        moves,
                        user_color,
                        user_accuracy: saved_accuracy,
                        eval_history,
                    };
                    // Empty puzzles + uci_moves so persist_completed_analysis
                    // knows this snapshot is already saved and skips re-saving.
                    *self.analysis_state.lock().unwrap() =
                        AnalysisState::Complete {
                            analysis,
                            puzzles: Vec::new(),
                            uci_moves: Vec::new(),
                            game_id: Some(game_id),
                        };
                    self.analysis_target_game_id = Some(game_id);
                    self.analysis_review_cursor = 0;
                }
            }
        }

        let replay = build_replay_state(game);
        self.replay_game = Some(replay);
    }

    // ── Save game to database ──────────────────────────────────────────
}
