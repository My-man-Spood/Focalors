use super::*;

/// Who the human plays in a new local game, picked on the setup card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SideChoice {
    White,
    Black,
    Random,
}

impl SideChoice {
    pub(crate) fn resolve(self) -> Color {
        match self {
            SideChoice::White => Color::White,
            SideChoice::Black => Color::Black,
            SideChoice::Random => {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                if nanos % 2 == 0 {
                    Color::White
                } else {
                    Color::Black
                }
            }
        }
    }

    /// String form persisted to settings.toml.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SideChoice::White => "white",
            SideChoice::Black => "black",
            SideChoice::Random => "random",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to White.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "black" => SideChoice::Black,
            "random" => SideChoice::Random,
            _ => SideChoice::White,
        }
    }
}

/// Clock preset for a new local game, picked on the setup card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimePreset {
    Bullet1_0,
    Bullet2_1,
    Blitz3_0,
    Blitz5_0,
    Blitz5_3,
    Rapid10_0,
    Rapid15_10,
    Classical30_0,
}

impl TimePreset {
    pub(crate) const ALL: [TimePreset; 8] = [
        TimePreset::Bullet1_0,
        TimePreset::Bullet2_1,
        TimePreset::Blitz3_0,
        TimePreset::Blitz5_0,
        TimePreset::Blitz5_3,
        TimePreset::Rapid10_0,
        TimePreset::Rapid15_10,
        TimePreset::Classical30_0,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            TimePreset::Bullet1_0 => "1+0",
            TimePreset::Bullet2_1 => "2+1",
            TimePreset::Blitz3_0 => "3+0",
            TimePreset::Blitz5_0 => "5+0",
            TimePreset::Blitz5_3 => "5+3",
            TimePreset::Rapid10_0 => "10+0",
            TimePreset::Rapid15_10 => "15+10",
            TimePreset::Classical30_0 => "30+0",
        }
    }

    /// String form persisted to settings.toml.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TimePreset::Bullet1_0 => "bullet_1_0",
            TimePreset::Bullet2_1 => "bullet_2_1",
            TimePreset::Blitz3_0 => "blitz_3_0",
            TimePreset::Blitz5_0 => "blitz_5_0",
            TimePreset::Blitz5_3 => "blitz_5_3",
            TimePreset::Rapid10_0 => "rapid_10_0",
            TimePreset::Rapid15_10 => "rapid_15_10",
            TimePreset::Classical30_0 => "classical_30_0",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to Rapid10_0.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "bullet_1_0" => TimePreset::Bullet1_0,
            "bullet_2_1" => TimePreset::Bullet2_1,
            "blitz_3_0" => TimePreset::Blitz3_0,
            "blitz_5_0" => TimePreset::Blitz5_0,
            "blitz_5_3" => TimePreset::Blitz5_3,
            "rapid_15_10" => TimePreset::Rapid15_10,
            "classical_30_0" => TimePreset::Classical30_0,
            _ => TimePreset::Rapid10_0,
        }
    }

    pub(crate) fn initial_ms(self) -> u64 {
        match self {
            TimePreset::Bullet1_0 => 60_000,
            TimePreset::Bullet2_1 => 120_000,
            TimePreset::Blitz3_0 => 180_000,
            TimePreset::Blitz5_0 => 300_000,
            TimePreset::Blitz5_3 => 300_000,
            TimePreset::Rapid10_0 => 600_000,
            TimePreset::Rapid15_10 => 900_000,
            TimePreset::Classical30_0 => 1_800_000,
        }
    }

    pub(crate) fn increment_ms(self) -> u64 {
        match self {
            TimePreset::Bullet2_1 => 1_000,
            TimePreset::Blitz5_3 => 3_000,
            TimePreset::Rapid15_10 => 10_000,
            _ => 0,
        }
    }
}

/// Opponent strength for a new local game, picked on the setup card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalDifficulty {
    Beginner,
    Club,
    Tournament,
    Master,
    Adaptive,
    Custom,
}

impl LocalDifficulty {
    pub(crate) const ALL: [LocalDifficulty; 6] = [
        LocalDifficulty::Beginner,
        LocalDifficulty::Club,
        LocalDifficulty::Tournament,
        LocalDifficulty::Master,
        LocalDifficulty::Adaptive,
        LocalDifficulty::Custom,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            LocalDifficulty::Beginner => "Beginner",
            LocalDifficulty::Club => "Club",
            LocalDifficulty::Tournament => "Tournament",
            LocalDifficulty::Master => "Master",
            LocalDifficulty::Adaptive => "Adaptive",
            LocalDifficulty::Custom => "Custom",
        }
    }

    /// String form persisted to settings.toml.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LocalDifficulty::Beginner => "beginner",
            LocalDifficulty::Club => "club",
            LocalDifficulty::Tournament => "tournament",
            LocalDifficulty::Master => "master",
            LocalDifficulty::Adaptive => "adaptive",
            LocalDifficulty::Custom => "custom",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to Tournament.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "beginner" => LocalDifficulty::Beginner,
            "club" => LocalDifficulty::Club,
            "master" => LocalDifficulty::Master,
            "adaptive" => LocalDifficulty::Adaptive,
            "custom" => LocalDifficulty::Custom,
            _ => LocalDifficulty::Tournament,
        }
    }

    pub(crate) fn numeric_level(self, adaptive_level: u32) -> u32 {
        match self {
            LocalDifficulty::Beginner => crate::strength::BEGINNER_LEVEL,
            LocalDifficulty::Club => crate::strength::CLUB_LEVEL,
            LocalDifficulty::Tournament => crate::strength::TOURNAMENT_LEVEL,
            LocalDifficulty::Master => crate::strength::MASTER_LEVEL,
            LocalDifficulty::Adaptive => adaptive_level,
            LocalDifficulty::Custom => crate::strength::MASTER_LEVEL,
        }
    }

    pub(crate) fn description(self, adaptive_level: u32) -> String {
        match self {
            LocalDifficulty::Beginner => "Fast, forgiving replies with shallow calculation.".into(),
            LocalDifficulty::Club => "Solid club-level play with restrained search depth.".into(),
            LocalDifficulty::Tournament => "Balanced default strength for serious local games.".into(),
            LocalDifficulty::Master => "Focalors' strongest local profile with deeper search.".into(),
            LocalDifficulty::Adaptive => format!(
                "Auto-adjusts to your level. Currently Level {} (~{} Elo).",
                adaptive_level,
                crate::strength::estimated_elo(adaptive_level),
            ),
            LocalDifficulty::Custom => "Thinks for a fixed time or to a fixed depth per move. Set it up under Advanced.".into(),
        }
    }
}

impl Default for LocalDifficulty {
    fn default() -> Self {
        Self::Tournament
    }
}

impl FocalorsApp {
// ── Welcome dialog (first launch) ────────────────────────────────

    pub(crate) fn draw_welcome_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_welcome {
            return;
        }

        egui::Window::new("Welcome to Focalors")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([360.0, 280.0])
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(
                    hydra_heading("Set up your profile", 16.0),
                );
                ui.add_space(4.0);
                    ui.label("This helps Focalors track your progress over time.");
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.label("Your name:");
                    ui.text_edit_singleline(&mut self.welcome_name);
                });

                ui.add_space(8.0);
                ui.label("Approximate rating:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.welcome_rating_choice, 800, "Beginner (~800)");
                    ui.selectable_value(&mut self.welcome_rating_choice, 1200, "Intermediate (~1200)");
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.welcome_rating_choice, 1600, "Advanced (~1600)");
                    ui.selectable_value(&mut self.welcome_rating_choice, 2000, "Expert (~2000)");
                });

                ui.add_space(16.0);
                let name_valid = !self.welcome_name.trim().is_empty();
                ui.add_enabled_ui(name_valid, |ui| {
                    if ui.button(hydra_heading("Get Started", 14.0)).clicked() {
                        if let Some(ref db) = self.db {
                            let name = self.welcome_name.trim().to_string();
                            if db.update_profile(&name, self.welcome_rating_choice).is_ok() {
                                self.profile = db.get_or_create_profile().ok();
                            }
                        }
                        self.show_welcome = false;
                    }
                });
                if !name_valid {
                    ui.label(
                        egui::RichText::new("Enter your name to continue")
                            .size(11.0)
                            .color(hydra_subtle_text()),
                    );
                }
            });
    }

    // ── Profile display (on home screen) ───────────────────────────────

    // ── Statistics dashboard ─────────────────────────────────────────

    pub(crate) fn draw_idle_home(&mut self, ui: &mut egui::Ui) {
        let searching = self.state.lock().unwrap().search_info.searching;

        let ctx = ui.ctx().clone();
        let status_label = "Offline local play ready".to_string();
        let has_puzzles = self
            .db
            .as_ref()
            .and_then(|db| db.get_puzzle_counts().ok())
            .map_or(false, |(total, _)| total > 0);
        let has_analyzed = self
            .db
            .as_ref()
            .and_then(|db| db.get_accuracy_history(1).ok())
            .map_or(false, |h| !h.is_empty());
        let has_games = !self.recent_games.is_empty();

        // Welcome strip on Overview — replaces the old generic "Focalors"
        // title + redundant Profile card. The KPI strip below already shows
        // Rating and Games, so a personal greeting with status badge is all
        // the orientation chrome the page needs.
        if self.home_page == HomePage::Overview {
            let profile_name = self
                .profile
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "there".to_string());
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.label(hydra_heading(format!("Welcome back, {profile_name}"), 26.0));
                    ui.label(
                        egui::RichText::new("Train, play, and review chess in one focused workspace.")
                            .size(12.0)
                            .color(hydra_subtle_text()),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    hydra_badge(ui, &status_label, hydra_panel_alt_fill());
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                            .size(10.0)
                            .color(hydra_subtle_text()),
                    );
                });
            });
        }
        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add(home_nav_button(self.home_page == HomePage::Overview, "Overview")).clicked() {
                self.home_page = HomePage::Overview;
            }

            if ui.add(home_nav_button(self.home_page == HomePage::Analyze, "Analyze")).clicked() {
                self.home_page = HomePage::Analyze;
            }

            let progress_response = ui.add_enabled(
                has_analyzed,
                home_nav_button(self.home_page == HomePage::Progress, "Progress"),
            );
            if progress_response.clicked() {
                self.home_page = HomePage::Progress;
            }

            let statistics_response = ui.add_enabled(
                has_games,
                home_nav_button(self.home_page == HomePage::Statistics, "Statistics"),
            );
            if statistics_response.clicked() {
                self.home_page = HomePage::Statistics;
            }

            let history_response = ui.add_enabled(
                has_games,
                home_nav_button(self.home_page == HomePage::History, "History"),
            );
            if history_response.clicked() {
                self.home_page = HomePage::History;
            }

            let puzzle_response = ui.add_enabled(
                has_puzzles || self.puzzle_trainer.is_some(),
                home_nav_button(self.home_page == HomePage::Puzzles, "Puzzles"),
            );
            if puzzle_response.clicked() {
                self.home_page = HomePage::Puzzles;
            }
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(14.0);

        // A puzzle session belongs to the Puzzles page. Leaving it by any
        // route ends the session exactly like the Exit button; otherwise the
        // trainer kept judging the moves of a game started afterwards and
        // reset the live board to the puzzle position.
        if self.home_page != HomePage::Puzzles {
            self.exit_puzzle_trainer();
        }

        match self.home_page {
            HomePage::Overview => {
                // ── KPI strip ──────────────────────────────────────────
                self.draw_home_kpi_strip(ui);
                ui.add_space(14.0);

                // ── Local play setup ───────────────────────────────────
                self.draw_local_setup_card(ui, searching);

                if let Some((ref msg, when)) = self.auto_adjust_message {
                    if when.elapsed().as_secs() < 8 {
                        ui.add_space(6.0);
                        hydra_callout_frame().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(msg)
                                    .size(12.0)
                                    .strong()
                                    .color(hydra_success()),
                            );
                        });
                    } else {
                        self.auto_adjust_message = None;
                    }
                }

                // ── Recent games + Your openings ───────────────────────
                if has_games {
                    ui.add_space(SECTION_GAP);
                    let total_w = ui.available_width();
                    if total_w > 760.0 {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = CARD_GAP;
                            ui.columns(2, |cols| {
                                hydra_card_frame().show(&mut cols[0], |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(OVERVIEW_ROW_H);
                                    self.draw_recent_games_compact(ui);
                                });
                                hydra_card_frame().show(&mut cols[1], |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(OVERVIEW_ROW_H);
                                    self.draw_opening_stats(ui);
                                });
                            });
                        });
                    } else {
                        hydra_card_frame().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            self.draw_recent_games_compact(ui);
                        });
                        ui.add_space(CARD_GAP);
                        hydra_card_frame().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            self.draw_opening_stats(ui);
                        });
                    }
                }
            }
            HomePage::Analyze => self.draw_analyze_page(ui),
            HomePage::Progress => self.draw_coaching_report(ui),
            HomePage::Statistics => self.draw_statistics_panel(ui),
            HomePage::History => self.draw_game_history(ui),
            HomePage::Puzzles => {
                if self.puzzle_trainer.is_some() {
                    self.draw_puzzle_trainer(ui);
                } else {
                    let (puzzle_total, puzzle_solved) = self
                        .db
                        .as_ref()
                        .and_then(|db| db.get_puzzle_counts().ok())
                        .unwrap_or((0, 0));
                    let weakest_theme = self
                        .db
                        .as_ref()
                        .and_then(|db| db.get_theme_stats().ok())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|(_, attempts, _)| *attempts >= 1)
                        .min_by(|a, b| {
                            let ra = a.2 as f64 / a.1 as f64;
                            let rb = b.2 as f64 / b.1 as f64;
                            ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(theme, attempts, solved)| {
                            format!(
                                "{} ({solved}/{attempts})",
                                crate::puzzles::PuzzleTheme::from_db_str(&theme).label()
                            )
                        });
                    let solve_rate = if puzzle_total > 0 {
                        format!("{:.0}% solve rate", puzzle_solved as f64 / puzzle_total as f64 * 100.0)
                    } else {
                        "no attempts yet".to_owned()
                    };
                    hydra_card_frame().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            hydra_heading("Puzzle Trainer", 16.0)
                                .color(hydra_accent()),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Drill positions extracted from your own analyzed games: every puzzle is a moment you got wrong.",
                            )
                            .size(12.0)
                            .color(hydra_subtle_text()),
                        );
                        ui.add_space(SECTION_GAP);
                        if has_puzzles {
                            ui.columns(3, |cols| {
                                hydra_fact(
                                    &mut cols[0],
                                    "SAVED PUZZLES",
                                    &format!("{puzzle_total}"),
                                    "waiting in your queue",
                                );
                                hydra_fact(
                                    &mut cols[1],
                                    "SOLVED",
                                    &format!("{puzzle_solved}"),
                                    &solve_rate,
                                );
                                hydra_fact(
                                    &mut cols[2],
                                    "WEAKEST THEME",
                                    weakest_theme.as_deref().unwrap_or("—"),
                                    "practice this one first",
                                );
                            });
                            ui.add_space(SECTION_GAP);
                            if ui
                                .add_sized([ui.available_width().min(280.0), 40.0], primary_button("Start Puzzle Session"))
                                .clicked()
                            {
                                self.start_puzzle_trainer();
                            }
                        } else {
                            ui.label(
                                egui::RichText::new(
                                    "No saved puzzles yet. Analyze finished games first so Focalors can extract training positions.",
                                )
                                .size(12.0)
                                .color(hydra_subtle_text()),
                            );
                        }
                    });
                }
            }
        }

        self.draw_custom_opponent_window(&ctx, searching);
    }

    /// Render a 4-card KPI strip at the top of the Overview: rating,
    /// accuracy, win rate, puzzles. Each tile shows a big value, a delta
    /// indicator vs. an earlier window, and a sparkline when there's
    /// enough history. Gracefully degrades on first-run / no-data state.
    pub(crate) fn draw_home_kpi_strip(&mut self, ui: &mut egui::Ui) {
        let (rating_history, accuracy_history, total_w, total_l, total_d, puzzle_total, puzzle_solved) = {
            let db = match &self.db {
                Some(d) => d,
                None => return,
            };
            (
                db.get_rating_history(50).ok().unwrap_or_default(),
                db.get_accuracy_history(50).ok().unwrap_or_default(),
                db.get_result_counts().ok().unwrap_or((0, 0, 0)).0,
                db.get_result_counts().ok().unwrap_or((0, 0, 0)).1,
                db.get_result_counts().ok().unwrap_or((0, 0, 0)).2,
                db.get_puzzle_counts().ok().unwrap_or((0, 0)).0,
                db.get_puzzle_counts().ok().unwrap_or((0, 0)).1,
            )
        };

        let total_games = total_w + total_l + total_d;
        let current_rating = rating_history
            .last()
            .map(|(_, _, r)| *r)
            .unwrap_or_else(|| self.profile.as_ref().map_or(1200, |p| p.rating));
        let rating_delta = if rating_history.len() >= 2 {
            let earlier_idx = rating_history.len().saturating_sub(10);
            current_rating - rating_history[earlier_idx].2
        } else {
            0
        };

        let (recent_acc, prior_acc, acc_spark) = if accuracy_history.is_empty() {
            (0.0_f64, 0.0_f64, Vec::<f64>::new())
        } else {
            let recent: Vec<f64> = accuracy_history.iter().rev().take(10).map(|(_, a)| *a).collect();
            let recent_avg = recent.iter().sum::<f64>() / recent.len() as f64;
            let prior: Vec<f64> = accuracy_history.iter().rev().skip(10).take(10).map(|(_, a)| *a).collect();
            let prior_avg = if prior.is_empty() {
                recent_avg
            } else {
                prior.iter().sum::<f64>() / prior.len() as f64
            };
            let spark: Vec<f64> = accuracy_history.iter().map(|(_, a)| *a).collect();
            (recent_avg, prior_avg, spark)
        };
        let acc_delta = recent_acc - prior_acc;

        let win_rate = if total_games > 0 {
            total_w as f64 / total_games as f64 * 100.0
        } else {
            0.0
        };
        let puzzle_rate = if puzzle_total > 0 {
            puzzle_solved as f64 / puzzle_total as f64 * 100.0
        } else {
            0.0
        };

        // Four equal KPI tiles, identical to the Statistics page so the two
        // read as one app. Trend lines live on Statistics, not here.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            ui.columns(4, |cols| {
                hydra_stat_tile(
                    &mut cols[0],
                    "RATING",
                    &format!("{current_rating}"),
                    (rating_delta != 0)
                        .then(|| (rating_delta > 0, format!("{}", rating_delta.abs()))),
                    &format!("{} rated games", rating_history.len()),
                );
                hydra_stat_tile(
                    &mut cols[1],
                    "ACCURACY",
                    &if acc_spark.is_empty() { "—".to_owned() } else { format!("{recent_acc:.1}%") },
                    (!acc_spark.is_empty() && acc_delta.abs() > 0.5)
                        .then(|| (acc_delta > 0.0, format!("{:.1}", acc_delta.abs()))),
                    "average of the last 10 analyzed",
                );
                hydra_stat_tile(
                    &mut cols[2],
                    "WIN RATE",
                    &if total_games > 0 { format!("{win_rate:.0}%") } else { "—".to_owned() },
                    None,
                    &format!("{total_w}W  {total_d}D  {total_l}L"),
                );
                hydra_stat_tile(
                    &mut cols[3],
                    "PUZZLES",
                    &if puzzle_total > 0 { format!("{puzzle_rate:.0}%") } else { "—".to_owned() },
                    None,
                    &format!("{puzzle_solved}/{puzzle_total} solved"),
                );
            });
        });
    }

    /// Compact "last 5 games" feed for the Overview right-side. Each row is
    /// a colored result chip + date + game info + a small "Review" button.
    /// Clicking Review loads the game into the Analyze tab and routes there.
    pub(crate) fn draw_recent_games_compact(&mut self, ui: &mut egui::Ui) {
        let games_snapshot: Vec<crate::db::SavedGame> = self
            .recent_games
            .iter()
            .take(5)
            .cloned()
            .collect();
        let mut replay_id: Option<i64> = None;
        ui.label(hydra_heading("Recent Games", 14.0));
        ui.add_space(8.0);
        if games_snapshot.is_empty() {
            ui.label(
                egui::RichText::new("No games yet — play one to fill this in.")
                    .size(11.0).color(hydra_subtle_text()),
            );
            return;
        }
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
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(" {result_label} "))
                        .color(hydra_text_on_accent())
                        .background_color(result_color)
                        .strong()
                        .monospace(),
                );
                ui.label(egui::RichText::new(date).size(11.0));
                ui.label(
                    egui::RichText::new(format!("as {} · {moves}m", game.user_color))
                        .size(10.0).color(hydra_subtle_text()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Review").clicked() {
                        replay_id = Some(game.id);
                    }
                });
            });
            ui.add_space(2.0);
        }
        if let Some(id) = replay_id {
            self.start_replay(id);
            self.home_page = HomePage::Analyze;
        }
    }

    pub(crate) fn draw_local_setup_card(&mut self, ui: &mut egui::Ui, searching: bool) {
        hydra_card_frame().show(ui, |ui| {
            ui.label(
                hydra_heading("Local Play", 16.0)
                    .color(hydra_accent()),
            );
            ui.label(
                egui::RichText::new(
                    "Choose your side, clock, and Focalors profile before entering a focused offline session.",
                )
                .size(12.0)
                .color(hydra_subtle_text()),
            );

            ui.add_space(10.0);
            egui::Grid::new("local_setup_form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Side")
                            .size(11.0)
                            .strong()
                            .color(hydra_subtle_text()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut self.local_side_choice, SideChoice::White, "White");
                        ui.radio_value(&mut self.local_side_choice, SideChoice::Black, "Black");
                        ui.radio_value(&mut self.local_side_choice, SideChoice::Random, "Random");
                    });
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("Time Control")
                            .size(11.0)
                            .strong()
                            .color(hydra_subtle_text()),
                    );
                    egui::ComboBox::from_id_salt("home_time_preset")
                        .selected_text(self.local_time_preset.label())
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            for preset in TimePreset::ALL {
                                ui.selectable_value(&mut self.local_time_preset, preset, preset.label());
                            }
                        });
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("Focalors Profile")
                            .size(11.0)
                            .strong()
                            .color(hydra_subtle_text()),
                    );
                    ui.horizontal(|ui| {
                        let prev_difficulty = self.local_difficulty;
                        egui::ComboBox::from_id_salt("home_local_difficulty")
                            .selected_text(self.local_difficulty.label())
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for difficulty in LocalDifficulty::ALL {
                                    ui.selectable_value(
                                        &mut self.local_difficulty,
                                        difficulty,
                                        difficulty.label(),
                                    );
                                }
                            });
                        if self.local_difficulty != prev_difficulty
                            && self.local_difficulty == LocalDifficulty::Custom
                        {
                            self.show_advanced_engine_settings = true;
                        }
                        if self.local_difficulty == LocalDifficulty::Custom
                            && ui.add(secondary_button("Advanced")).clicked()
                        {
                            self.show_advanced_engine_settings = true;
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(self.local_difficulty.description(self.adaptive_level))
                    .size(11.0)
                    .color(hydra_subtle_text()),
            );

            ui.add_space(10.0);
            let start_clicked = ui
                .add_enabled_ui(!searching, |ui| {
                    ui.add_sized([ui.available_width(), 38.0], primary_button("Start Local Game"))
                        .clicked()
                })
                .inner;
            if start_clicked {
                self.begin_local_game(self.local_side_choice.resolve());
            }
        });
    }
}
