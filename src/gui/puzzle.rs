use super::*;

impl FocalorsApp {
    /// Ends the puzzle session, if any. Called by the Exit button, by
    /// leaving the Puzzles page through the navigation, and by every new
    /// game, so a puzzle left open can never judge a game's moves.
    pub(crate) fn exit_puzzle_trainer(&mut self) {
        if self.puzzle_trainer.is_none() {
            return;
        }
        let mut state = self.state.lock().unwrap();
        end_puzzle_session(&mut self.puzzle_trainer, &mut self.puzzle_message, &mut state);
    }

    pub(crate) fn start_puzzle_trainer(&mut self) {
        let db = match &self.db {
            Some(d) => d,
            None => return,
        };
        let puzzles = match db.get_training_puzzles(1) {
            Ok(p) if !p.is_empty() => p,
            _ => return,
        };
        let puzzle = puzzles.into_iter().next().unwrap();
        self.home_page = HomePage::Puzzles;
        self.load_puzzle(puzzle);
    }

    pub(crate) fn load_puzzle(&mut self, puzzle: crate::db::SavedPuzzle) {
        let board = match Board::from_fen(&puzzle.fen) {
            Ok(b) => b,
            Err(_) => return,
        };
        let solution_move = match crate::uci::parse_move(&board, &puzzle.solution) {
            Some(m) => m,
            None => return,
        };
        // Set the shared board to the puzzle position
        self.state.lock().unwrap().board = board.clone();
        let board_copy = board.clone();
        self.puzzle_trainer = Some(PuzzleTrainerState {
            puzzle,
            board: board_copy,
            solution_move,
            user_attempted: false,
            user_solved: false,
            show_hint: false,
            show_answer: false,
            wrong_attempts: 0,
        });
        self.puzzle_message = None;
        self.flipped = board.side_to_move == Color::Black;
    }

    pub(crate) fn draw_puzzle_trainer(&mut self, ui: &mut egui::Ui) {
        // Extract display data upfront to avoid borrow conflicts
        let (theme_str, rating, stm_white, solved, show_answer, show_hint,
             wrong_attempts, solution_uci) = {
            let t = match &self.puzzle_trainer {
                Some(t) => t,
                None => return,
            };
            (
                t.puzzle.theme.clone(),
                t.puzzle.rating,
                t.board.side_to_move == Color::White,
                t.user_solved,
                t.show_answer,
                t.show_hint,
                t.wrong_attempts,
                t.solution_move.to_uci(),
            )
        };

        let theme_stats = self.db.as_ref()
            .and_then(|db| db.get_theme_stats().ok())
            .unwrap_or_default();

        let mut exit_clicked = false;
        let mut hint_clicked = false;
        let mut answer_clicked = false;
        let mut next_clicked = false;

        // Same shape as Game Review: the board on the left, a panel card on
        // the right, the pair centered on wide screens.
        ui.horizontal_top(|ui| {
            let gap_w: f32 = 12.0;
            let right_w: f32 = 380.0;
            let avail = ui.available_width();
            let board_w: f32 = (avail - gap_w - right_w).clamp(320.0, 720.0);
            let total = board_w + gap_w + right_w;
            if avail > total {
                ui.add_space((avail - total) / 2.0);
            }

            ui.vertical(|ui| {
                ui.set_max_width(board_w);
                self.draw_board(ui, None);
            });

            ui.add_space(gap_w);

            ui.vertical(|ui| {
                ui.set_max_width(right_w);
                hydra_card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(hydra_heading("Puzzle Trainer", 15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Exit").clicked() {
                                exit_clicked = true;
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if let Some(ref ts) = theme_str {
                            let theme = crate::puzzles::PuzzleTheme::from_db_str(ts);
                            hydra_badge(ui, theme.label(), hydra_accent());
                        }
                        if let Some(r) = rating {
                            ui.label(
                                egui::RichText::new(format!("Rating {r}"))
                                    .size(11.0)
                                    .color(hydra_subtle_text()),
                            );
                        }
                    });
                    ui.add_space(SECTION_GAP);

                    let stm = if stm_white { "White" } else { "Black" };
                    ui.label(hydra_heading(format!("{stm} to move"), 14.0));
                    ui.label(
                        egui::RichText::new("Find the best move on the board.")
                            .size(12.0)
                            .color(hydra_subtle_text()),
                    );

                    if let Some((ref msg, color, when)) = self.puzzle_message {
                        if when.elapsed().as_secs() < 5 {
                            ui.add_space(8.0);
                            ui.label(hydra_heading(msg.clone(), 13.0).color(color));
                        }
                    }
                    ui.add_space(SECTION_GAP);

                    if !solved && !show_answer {
                        ui.horizontal(|ui| {
                            if !show_hint && ui.button("Show Hint").clicked() {
                                hint_clicked = true;
                            }
                            if wrong_attempts >= 1 && ui.button("Show Answer").clicked() {
                                answer_clicked = true;
                            }
                        });
                    }
                    if (solved || show_answer)
                        && ui
                            .add_sized([ui.available_width(), 38.0], primary_button("Next Puzzle"))
                            .clicked()
                    {
                        next_clicked = true;
                    }

                    if !theme_stats.is_empty() {
                        ui.add_space(SECTION_GAP);
                        ui.label(hydra_heading("SOLVE RATES", 10.0).color(hydra_subtle_text()));
                        ui.add_space(4.0);
                        for (theme, attempts, solved) in &theme_stats {
                            let label = crate::puzzles::PuzzleTheme::from_db_str(theme).label();
                            let rate = if *attempts > 0 {
                                format!("{solved}/{attempts} · {:.0}%", *solved as f64 / *attempts as f64 * 100.0)
                            } else {
                                "0/0".to_string()
                            };
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(label).size(11.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(rate)
                                            .size(11.0)
                                            .color(hydra_subtle_text()),
                                    );
                                });
                            });
                        }
                    }
                });
            });
        });

        // Process deferred actions
        if exit_clicked {
            self.exit_puzzle_trainer();
        }
        if hint_clicked {
            if let Some(ref ts) = theme_str {
                let theme = crate::puzzles::PuzzleTheme::from_db_str(ts);
                self.puzzle_message = Some((
                    theme.hint().to_string(),
                    hydra_warning(),
                    std::time::Instant::now(),
                ));
            }
            if let Some(ref mut t) = self.puzzle_trainer {
                t.show_hint = true;
            }
        }
        if answer_clicked {
            self.puzzle_message = Some((
                format!("Answer: {solution_uci}"),
                class_mistake(),
                std::time::Instant::now(),
            ));
            if let Some(ref mut t) = self.puzzle_trainer {
                t.show_answer = true;
            }
        }
        if next_clicked {
            self.start_puzzle_trainer();
        }
    }

    /// Handle a move made during puzzle solving.
    pub(crate) fn handle_puzzle_move(&mut self, played_move: Move, animate: bool) {
        // Extract what we need before mutating
        let (solution_move, puzzle_id, puzzle_board, is_solved, is_shown) = {
            let t = match &self.puzzle_trainer {
                Some(t) => t,
                None => return,
            };
            (t.solution_move, t.puzzle.id, t.board.clone(), t.user_solved, t.show_answer)
        };
        if is_solved || is_shown {
            return;
        }

        let is_correct = played_move == solution_move;

        if is_correct {
            if let Some(ref db) = self.db {
                let _ = db.record_puzzle_attempt(puzzle_id, true);
            }
            self.puzzle_message = Some((
                "Correct!".to_string(),
                hydra_success(),
                std::time::Instant::now(),
            ));
            let mut board = puzzle_board;
            let anim = pending_animation(&board, played_move, animate);
            make_move(&mut board, played_move);
            {
                let mut s = self.state.lock().unwrap();
                s.board = board;
                if let Some(anim) = anim {
                    s.pending_animations.push(anim);
                }
            }
            if let Some(ref mut t) = self.puzzle_trainer {
                t.user_solved = true;
                t.user_attempted = true;
            }
        } else {
            if let Some(ref mut t) = self.puzzle_trainer {
                t.wrong_attempts += 1;
                t.user_attempted = true;
            }
            if let Some(ref db) = self.db {
                let _ = db.record_puzzle_attempt(puzzle_id, false);
            }
            self.puzzle_message = Some((
                "Not quite. Try again!".to_string(),
                hydra_danger(),
                std::time::Instant::now(),
            ));
            self.state.lock().unwrap().board = puzzle_board;
        }
    }

    // ── Game history panel ──────────────────────────────────────────────
}
