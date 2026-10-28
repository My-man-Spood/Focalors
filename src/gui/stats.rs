use super::*;

impl FocalorsApp {
    pub(crate) fn draw_statistics_panel(&mut self, ui: &mut egui::Ui) {
        let db = match &self.db {
            Some(d) => d,
            None => return,
        };

        // Gather all data upfront
        let rating_history = db.get_rating_history(50).ok().unwrap_or_default();
        let accuracy_history = db.get_accuracy_history(50).ok().unwrap_or_default();
        let (total_w, total_l, total_d) = db.get_result_counts().ok().unwrap_or((0, 0, 0));
        let by_color = db.get_results_by_color().ok().unwrap_or(((0,0,0),(0,0,0)));
        let by_tc = db.get_results_by_time_control().ok().unwrap_or_default();
        let (phase_o, phase_m, phase_e) = db.get_phase_weakness(20).ok().unwrap_or((0, 0, 0));
        let theme_stats = db.get_theme_stats().ok().unwrap_or_default();
        let (puzzle_total, puzzle_solved) = db.get_puzzle_counts().ok().unwrap_or((0, 0));

        ui.horizontal(|ui| {
            ui.label(hydra_heading("Statistics", 22.0));
            ui.label(egui::RichText::new("Last 50 games").color(hydra_subtle_text()));
        });
        ui.add_space(16.0);

        // ── KPI strip ────────────────────────────────────────────────
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

        // ── Row 1: four equal KPI tiles ──────────────────────────────
        // Every tile gets the same treatment (label, value, delta, sub-line);
        // the trend lines live in the charts below instead of being
        // duplicated as sparklines here.
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
        ui.add_space(SECTION_GAP);

        // ── Row 2: Rating chart + Results ────────────────────────────
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            ui.columns(2, |cols| {
                hydra_card_frame().show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ROW_CARD_H);
                    ui.label(hydra_heading("Rating Over Time", 14.0));
                    ui.add_space(6.0);
                    if rating_history.len() >= 2 {
                        let points: Vec<[f64; 2]> = rating_history
                            .iter()
                            .enumerate()
                            .map(|(i, (_, _, after))| [i as f64 + 1.0, *after as f64])
                            .collect();
                        let lo = rating_history.iter().map(|(_, _, r)| *r).min().unwrap_or(1200);
                        let hi = rating_history.iter().map(|(_, _, r)| *r).max().unwrap_or(1200);
                        // Pad relative to the actual spread so small swings are
                        // still readable instead of a flat line in a huge range.
                        let pad = ((hi - lo) as f64 * 0.35).max(25.0);
                        let floor = lo as f64 - pad;
                        let line = egui_plot::Line::new("rating", egui_plot::PlotPoints::new(points))
                            .color(hydra_accent())
                            .width(2.0_f32)
                            .fill(floor as f32)
                            .fill_alpha(0.14_f32);
                        egui_plot::Plot::new("rating_chart")
                            .height(230.0)
                            .include_y(floor)
                            .include_y(hi as f64 + pad)
                            .allow_drag(false)
                            .allow_zoom(false)
                            .allow_scroll(false)
                            .show_x(false)
                            .show_y(false)
                            .show_background(false)
                            .show_grid([false, true])
                            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [25.0, 50.0, 100.0]))
                            .show(ui, |plot_ui| {
                                plot_ui.line(line);
                            });
                    } else {
                        ui.add_space(80.0);
                        ui.label(
                            egui::RichText::new("Play a few games to see your rating trend.")
                                .color(hydra_subtle_text()),
                        );
                    }
                });
                hydra_card_frame().show(&mut cols[1], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ROW_CARD_H);
                    ui.label(hydra_heading("Results", 14.0));
                    ui.add_space(8.0);
                    if total_games > 0 {
                        let ((ww, wl, wd), (bw, bl, bd)) = by_color;
                        draw_result_bar(ui, "Overall", total_w as u32, total_l as u32, total_d as u32);
                        // Per-color bars only add information once both colors
                        // have games; otherwise they just repeat "Overall".
                        if (ww + wl + wd) > 0 && (bw + bl + bd) > 0 {
                            draw_result_bar(ui, "As White", ww as u32, wl as u32, wd as u32);
                            draw_result_bar(ui, "As Black", bw as u32, bl as u32, bd as u32);
                        }
                        let tc_games: i64 = by_tc.iter().map(|(_, w, l, d)| (*w + *l + *d) as i64).sum();
                        if !by_tc.is_empty() && (by_tc.len() > 1 || tc_games != total_games as i64) {
                            ui.add_space(8.0);
                            ui.label(
                                hydra_heading("By Time Control", 11.0).color(hydra_subtle_text()),
                            );
                            for (tc, w, l, d) in &by_tc {
                                draw_result_bar(ui, tc, *w as u32, *l as u32, *d as u32);
                            }
                        }
                    } else {
                        ui.add_space(80.0);
                        ui.label(
                            egui::RichText::new("No completed games yet.").color(hydra_subtle_text()),
                        );
                    }
                });
            });
        });
        ui.add_space(SECTION_GAP);

        // ── Row 3: Accuracy chart + Phase weakness ───────────────────
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            ui.columns(2, |cols| {
                hydra_card_frame().show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ROW_CARD_H);
                    ui.horizontal(|ui| {
                        ui.label(hydra_heading("Accuracy Trends", 14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            chart_legend_row(
                                ui,
                                &[(hydra_subtle_text(), "per game"), (hydra_accent(), "5-game avg")],
                            );
                        });
                    });
                    ui.add_space(6.0);
                    if accuracy_history.len() >= 2 {
                        let acc_points: Vec<[f64; 2]> = accuracy_history
                            .iter()
                            .enumerate()
                            .map(|(i, (_, acc))| [i as f64 + 1.0, *acc])
                            .collect();
                        let rolling: Vec<[f64; 2]> = accuracy_history
                            .iter()
                            .enumerate()
                            .map(|(i, _)| {
                                let start = i.saturating_sub(4);
                                let window = &accuracy_history[start..=i];
                                let avg = window.iter().map(|(_, a)| a).sum::<f64>() / window.len() as f64;
                                [(i + 1) as f64, avg]
                            })
                            .collect();
                        let best = accuracy_history
                            .iter()
                            .map(|(_, a)| *a)
                            .fold(f64::MIN, f64::max);
                        let lowest = accuracy_history
                            .iter()
                            .map(|(_, a)| *a)
                            .fold(f64::MAX, f64::min);
                        // Floor at the nearest 10 below the worst game (never
                        // below 0) so the lines use the card instead of
                        // hugging the top of a fixed 0-100 box.
                        let floor = ((lowest / 10.0).floor() * 10.0 - 10.0).max(0.0);
                        let line_acc = egui_plot::Line::new("accuracy", egui_plot::PlotPoints::new(acc_points))
                            .color(hydra_subtle_text())
                            .width(1.0_f32);
                        let line_avg = egui_plot::Line::new("rolling_avg", egui_plot::PlotPoints::new(rolling))
                            .color(hydra_accent())
                            .width(2.0_f32)
                            .fill(floor as f32)
                            .fill_alpha(0.14_f32);
                        let best_line = egui_plot::HLine::new("best", best)
                            .color(class_best())
                            .width(1.0_f32)
                            .style(egui_plot::LineStyle::Dashed { length: 6.0 });
                        egui_plot::Plot::new("accuracy_chart")
                            .height(210.0)
                            .include_y(floor)
                            .include_y(100.0)
                            .allow_drag(false)
                            .allow_zoom(false)
                            .allow_scroll(false)
                            .show_x(false)
                            .show_y(false)
                            .show_background(false)
                            .show_grid([false, true])
                            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [25.0, 50.0, 100.0]))
                            .show(ui, |plot_ui| {
                                plot_ui.hline(best_line);
                                plot_ui.line(line_acc);
                                plot_ui.line(line_avg);
                            });
                        ui.label(
                            egui::RichText::new(format!("Personal best: {best:.1}%"))
                                .size(11.0).color(hydra_subtle_text()),
                        );
                    } else {
                        ui.add_space(80.0);
                        ui.label(
                            egui::RichText::new("Analyze games to see accuracy trends.")
                                .color(hydra_subtle_text()),
                        );
                    }
                });
                hydra_card_frame().show(&mut cols[1], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ROW_CARD_H);
                    ui.label(hydra_heading("Phase Weakness", 14.0));
                    ui.add_space(8.0);
                    let total_phase_errors = phase_o + phase_m + phase_e;
                    if total_phase_errors > 0 {
                        let max_count = phase_o.max(phase_m).max(phase_e);
                        let phases = [
                            ("Opening", phase_o, "moves 1-15"),
                            ("Middlegame", phase_m, "moves 16-35"),
                            ("Endgame", phase_e, "moves 36+"),
                        ];
                        for (label, count, sub) in &phases {
                            let pct = *count as f64 / total_phase_errors as f64;
                            // Bars share one hue so length is the only signal;
                            // the weakest phase is named instead of recolored.
                            let color = hydra_accent();
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(hydra_heading(*label, 12.0));
                                if *count == max_count {
                                    ui.add_space(4.0);
                                    hydra_badge(ui, "WEAKEST", hydra_danger());
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{count} errors"))
                                            .size(11.0).color(hydra_subtle_text()),
                                    );
                                });
                            });
                            ui.label(egui::RichText::new(*sub).size(10.0).color(hydra_subtle_text()));
                            let bar_w = ui.available_width();
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(bar_w, 10.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(rect, 5.0, hydra_panel_alt_fill());
                            let fill_w = bar_w * pct as f32;
                            if fill_w > 0.0 {
                                let fill_rect = egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(fill_w, 10.0),
                                );
                                ui.painter().rect_filled(fill_rect, 5.0, color);
                            }
                        }
                    } else {
                        ui.add_space(80.0);
                        ui.label(
                            egui::RichText::new("Analyze games to see phase weaknesses.")
                                .color(hydra_subtle_text()),
                        );
                    }
                });
            });
        });
        ui.add_space(SECTION_GAP);

        // ── Row 4: Puzzle theme heatmap (naked section) ────────────────
        if !theme_stats.is_empty() {
            hydra_card_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(hydra_heading("Puzzle Themes", 14.0));
                    ui.label(
                        egui::RichText::new("(sorted weakest first)")
                            .size(10.0).color(hydra_subtle_text()),
                    );
                });
                ui.add_space(8.0);
                let mut themes: Vec<_> = theme_stats.iter().filter(|(_, a, _)| *a >= 1).collect();
                themes.sort_by(|a, b| {
                    let ra = a.2 as f64 / a.1 as f64;
                    let rb = b.2 as f64 / b.1 as f64;
                    ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                });
                ui.horizontal_wrapped(|ui| {
                    for (theme, attempts, solved) in &themes {
                        let rate = *solved as f64 / *attempts as f64;
                        let color = if rate < 0.4 {
                            class_blunder()
                        } else if rate < 0.6 {
                            class_mistake()
                        } else if rate < 0.75 {
                            class_inaccuracy()
                        } else {
                            class_best()
                        };
                        let label = crate::puzzles::PuzzleTheme::from_db_str(theme).label();
                        let tile = egui::Frame::new()
                            .fill(color.gamma_multiply(0.18))
                            .stroke(egui::Stroke::new(1.0_f32, color))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::same(8));
                        tile.show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(label).size(11.0).strong());
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.0}% · {solved}/{attempts}",
                                        rate * 100.0
                                    ))
                                    .size(10.0).color(hydra_subtle_text()),
                                );
                            });
                        });
                    }
                });
            });
            ui.add_space(SECTION_GAP);
        }

        // ── Session Summary (compact naked footer) ────────────────────
        let current_games = self.profile.as_ref().map_or(0, |p| p.games_played);
        let current_rating_p = self.profile.as_ref().map_or(1200, |p| p.rating);
        let session_games = current_games - self.session_start_games;
        let session_rating_delta = current_rating_p - self.session_start_rating;
        if session_games > 0 || self.session_best_accuracy.is_some() {
            ui.horizontal_wrapped(|ui| {
                ui.label(hydra_heading("This Session", 13.0));
                ui.label(
                    egui::RichText::new(format!("· Games: {session_games}"))
                        .size(11.0).color(hydra_subtle_text()),
                );
                if session_rating_delta != 0 {
                    let sign = if session_rating_delta >= 0 { "+" } else { "" };
                    let color = if session_rating_delta > 0 {
                        hydra_success()
                    } else if session_rating_delta < 0 {
                        hydra_danger()
                    } else {
                        hydra_text()
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "· Rating: {current_rating_p} ({sign}{session_rating_delta})"
                        ))
                        .size(11.0).color(color),
                    );
                }
                if let Some(best) = self.session_best_accuracy {
                    ui.label(
                        egui::RichText::new(format!("· Best accuracy: {best:.1}%"))
                            .size(11.0).color(accuracy_color(best)),
                    );
                }
            });
        }
    }

    // ── Opening stats ───────────────────────────────────────────────

    pub(crate) fn draw_opening_stats(&mut self, ui: &mut egui::Ui) {
        let stats = match &self.db {
            Some(db) => db.get_opening_stats().ok().unwrap_or_default(),
            None => return,
        };
        if stats.is_empty() {
            return;
        }

        ui.label(hydra_heading("Your Openings", 14.0));
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .id_salt("overview_openings")
            .max_height(OPENINGS_LIST_MAX_H)
            .auto_shrink([false, true])
            .show(ui, |ui| {
            for (name, total, wins, losses, draws) in &stats {
                let win_rate = if *total > 0 { *wins as f64 / *total as f64 * 100.0 } else { 0.0 };
                let color = if win_rate >= 60.0 {
                    hydra_success()
                } else if win_rate <= 35.0 {
                    hydra_danger()
                } else {
                    hydra_text()
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{total}g  W{wins}/L{losses}/D{draws}  ({win_rate:.0}%)"
                            ))
                            .size(10.0)
                            .color(color),
                        );
                    });
                });
            }
            });
    }

    // ── Eval explanation panel ────────────────────────────────────────

    pub(crate) fn draw_eval_panel(&mut self, ui: &mut egui::Ui) {
        let board = self.state.lock().unwrap().board.clone();
        let breakdown = crate::eval::eval_components(&board);

        hydra_card_frame().show(ui, |ui| {
            let total_pawns = breakdown.total as f64 / 100.0;
            let sign = if total_pawns >= 0.0 { "+" } else { "" };
            ui.label(
                hydra_heading(format!("Eval: {sign}{total_pawns:.2}"), 14.0)
                    .color(if breakdown.total > 50 {
                        hydra_success()
                    } else if breakdown.total < -50 {
                        hydra_danger()
                    } else {
                        hydra_text()
                    }),
            );
            ui.add_space(4.0);

            let components = [
                ("Material", breakdown.material),
                ("Piece Placement", breakdown.pst),
                ("Mobility", breakdown.mobility),
                ("Pawn Structure", breakdown.pawn_structure),
                ("Passed Pawns", breakdown.passed_pawns),
                ("King Safety", breakdown.king_safety),
                ("Bishop Pair", breakdown.bishop_pair),
                ("Rook Placement", breakdown.rook_placement),
                ("Knight Outposts", breakdown.knight_outpost),
                ("Connected Passers", breakdown.connected_passers),
                ("King-Pawn Prox.", breakdown.king_pawn_proximity),
                ("Tempo", breakdown.tempo),
            ];

            for (name, value) in &components {
                if *value == 0 {
                    continue; // skip zero components for cleanliness
                }
                let pawns = *value as f64 / 100.0;
                let sign = if pawns >= 0.0 { "+" } else { "" };
                let color = if *value > 30 {
                    hydra_success()
                } else if *value < -30 {
                    hydra_danger()
                } else {
                    hydra_subtle_text()
                };

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{name}:"))
                            .size(10.0)
                            .color(hydra_subtle_text()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut text = format!("{sign}{pawns:.2}");
                        // Human-readable annotation for significant values
                        if name == &"Material" && value.abs() > 200 {
                            let pieces = value.abs() / 100;
                            text = format!("{sign}{pawns:.2} ({pieces} pawn{})", if pieces > 1 { "s" } else { "" });
                        } else if name == &"King Safety" && *value < -100 {
                            text = format!("{sign}{pawns:.2} (exposed)");
                        } else if name == &"Bishop Pair" && *value > 30 {
                            text = format!("{sign}{pawns:.2} (advantage)");
                        }
                        ui.label(egui::RichText::new(text).size(10.0).color(color));
                    });
                });
            }
        });
    }

    // ── Coaching report ──────────────────────────────────────────────

    pub(crate) fn draw_coaching_report(&mut self, ui: &mut egui::Ui) {
        let db = match &self.db {
            Some(d) => d,
            None => return,
        };

        let accuracy_history = db.get_accuracy_history(20).ok().unwrap_or_default();
        let classification_stats = db.get_classification_stats(10).ok().unwrap_or_default();
        let phase_weakness = db.get_phase_weakness(10).ok().unwrap_or((0, 0, 0));
        let theme_stats = db.get_theme_stats().ok().unwrap_or_default();

        ui.horizontal(|ui| {
            ui.label(hydra_heading("Progress", 22.0));
            ui.label(egui::RichText::new("Last 10 games").color(hydra_subtle_text()));
        });
        ui.add_space(16.0);

        // Empty-state for the whole tab — no analysis yet.
        if accuracy_history.is_empty() {
            hydra_card_frame().show(ui, |ui| {
                ui.set_min_height(140.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(hydra_heading("No analyzed games yet", 16.0));
                    ui.label(
                        egui::RichText::new("Play a game and click \"Analyze Game\" to see your progress.")
                            .color(hydra_subtle_text()),
                    );
                });
            });
            return;
        }

        // Accuracy stats from the most-recent vs prior 10 analyzed games.
        let recent_avg = {
            let recent: Vec<f64> = accuracy_history.iter().rev().take(10).map(|(_, a)| *a).collect();
            recent.iter().sum::<f64>() / recent.len() as f64
        };
        let prior_avg = {
            let prior: Vec<f64> = accuracy_history.iter().rev().skip(10).take(10).map(|(_, a)| *a).collect();
            if prior.is_empty() {
                recent_avg
            } else {
                prior.iter().sum::<f64>() / prior.len() as f64
            }
        };
        let acc_delta = recent_avg - prior_avg;

        // ── Row 1: Accuracy gauge + Move breakdown ───────────────────
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            ui.columns(2, |cols| {
                hydra_card_frame().show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(PROGRESS_ROW_H);
                    ui.label(hydra_heading("Recent Accuracy", 14.0));
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        draw_radial_gauge(ui, 184.0, recent_avg, hydra_accent());
                        ui.add_space(6.0);
                        if accuracy_history.len() >= 11 && acc_delta.abs() > 0.5 {
                            let color = if acc_delta > 0.0 { hydra_success() } else { hydra_danger() };
                            let arrow = if acc_delta > 0.0 { "▲" } else { "▼" };
                            ui.label(
                                egui::RichText::new(format!("{arrow} {:.1}% vs prior 10", acc_delta.abs()))
                                    .size(12.0).color(color).strong(),
                            );
                        } else if accuracy_history.len() >= 11 {
                            ui.label(
                                egui::RichText::new("≈ stable vs prior 10")
                                    .size(12.0).color(hydra_subtle_text()),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!("{} analyzed games", accuracy_history.len()))
                                    .size(12.0).color(hydra_subtle_text()),
                            );
                        }
                    });
                });
                hydra_card_frame().show(&mut cols[1], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(PROGRESS_ROW_H);
                    ui.label(hydra_heading("Move Breakdown", 14.0));
                    ui.label(
                        egui::RichText::new("Across the last 10 analyzed games")
                            .size(10.0).color(hydra_subtle_text()),
                    );
                    ui.add_space(12.0);
                    if classification_stats.is_empty() {
                        ui.add_space(60.0);
                        ui.label(
                            egui::RichText::new("No classified moves yet.").color(hydra_subtle_text()),
                        );
                    } else {
                        let lookup = |class: &str| -> i32 {
                            classification_stats
                                .iter()
                                .find(|(c, _)| c == class)
                                .map_or(0, |(_, n)| *n)
                        };
                        let rows = [
                            ("Best", lookup("best"), class_best()),
                            ("Good", lookup("good"), class_good()),
                            ("Book", lookup("book"), class_book()),
                            ("Inaccuracy", lookup("inaccuracy"), class_inaccuracy()),
                            ("Mistake", lookup("mistake"), class_mistake()),
                            ("Blunder", lookup("blunder"), class_blunder()),
                        ];
                        let total: i32 = rows.iter().map(|(_, n, _)| *n).sum();
                        if total > 0 {
                            // Tight rows: six classes must fit beside the gauge.
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for (label, n, color) in &rows {
                                if *n == 0 {
                                    continue;
                                }
                                let pct = *n as f32 / total as f32;
                                ui.horizontal(|ui| {
                                    ui.label(hydra_heading(*label, 11.0).color(*color));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{n}"))
                                                    .size(11.0).color(hydra_subtle_text()),
                                            );
                                        },
                                    );
                                });
                                let bar_w = ui.available_width();
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(bar_w, 6.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 3.0, hydra_panel_alt_fill());
                                let fill_w = bar_w * pct;
                                if fill_w > 0.0 {
                                    let fill_rect = egui::Rect::from_min_size(
                                        rect.min,
                                        egui::vec2(fill_w, 6.0),
                                    );
                                    ui.painter().rect_filled(fill_rect, 3.0, *color);
                                }
                                ui.add_space(8.0);
                            }
                        }
                    }
                });
            });
        });
        ui.add_space(SECTION_GAP);

        // ── Row 2: Phase breakdown, one card with three neutral columns ──
        // Bar/tile tinting is gone: the weakest phase is named by a badge
        // and its count turns red; everything else stays in text color.
        let (opening, middle, endgame) = phase_weakness;
        let total_phase_errors = opening + middle + endgame;
        hydra_card_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(hydra_heading("Phase Breakdown", 14.0));
            ui.label(
                egui::RichText::new("Where your errors happen")
                    .size(10.0).color(hydra_subtle_text()),
            );
            ui.add_space(12.0);
            if total_phase_errors > 0 {
                let max_count = opening.max(middle).max(endgame);
                let phases = [
                    ("Opening", opening, "moves 1-15"),
                    ("Middlegame", middle, "moves 16-35"),
                    ("Endgame", endgame, "moves 36+"),
                ];
                ui.columns(3, |cols| {
                    for (i, (label, count, sub)) in phases.iter().enumerate() {
                        let pct = *count as f64 / total_phase_errors as f64;
                        let weakest = *count == max_count && max_count > 0;
                        let ui = &mut cols[i];
                        ui.horizontal(|ui| {
                            ui.label(hydra_heading(*label, 12.0));
                            if weakest {
                                ui.add_space(4.0);
                                hydra_badge(ui, "WEAKEST", hydra_danger());
                            }
                        });
                        ui.label(
                            egui::RichText::new(*sub).size(10.0).color(hydra_subtle_text()),
                        );
                        ui.add_space(6.0);
                        let count_text = hydra_heading(format!("{count}"), 26.0);
                        ui.label(if weakest { count_text.color(hydra_danger()) } else { count_text });
                        ui.label(
                            egui::RichText::new(format!("{:.0}% of errors", pct * 100.0))
                                .size(10.0).color(hydra_subtle_text()),
                        );
                    }
                });
            } else {
                ui.label(
                    egui::RichText::new("No phase errors recorded yet.")
                        .color(hydra_subtle_text()),
                );
            }
        });
        ui.add_space(SECTION_GAP);

        // ── Row 3: Coach tips ─────────────────────────────────────────
        let weak_themes: Vec<_> = theme_stats
            .iter()
            .filter(|(_, attempts, solved)| *attempts >= 3 && (*solved as f64 / *attempts as f64) < 0.5)
            .collect();
        let blunders = classification_stats
            .iter()
            .find(|(c, _)| c == "blunder")
            .map_or(0, |(_, n)| *n);

        let mut tips: Vec<(String, egui::Color32)> = Vec::new();

        if total_phase_errors > 0 {
            let (weakest_name, action) = if opening >= middle && opening >= endgame {
                ("opening", "Focus on piece development and central control in the first 15 moves.")
            } else if middle >= endgame {
                ("middlegame", "Work on tactical awareness and piece coordination through moves 16-35.")
            } else {
                ("endgame", "Practice king activity and passed-pawn technique in the endgame.")
            };
            tips.push((
                format!("Your {weakest_name} is your weakest phase. {action}"),
                class_mistake(),
            ));
        }

        if blunders > 5 {
            tips.push((
                "You're blundering frequently - slow down and double-check captures and checks before moving."
                    .into(),
                class_blunder(),
            ));
        }

        if !weak_themes.is_empty() {
            let names: Vec<_> = weak_themes
                .iter()
                .map(|(t, _, _)| crate::puzzles::PuzzleTheme::from_db_str(t).label())
                .collect();
            tips.push((
                format!(
                    "Weak puzzle themes: {}. Use the puzzle trainer to practice.",
                    names.join(", ")
                ),
                hydra_accent(),
            ));
        }

        if let Some(ref p) = self.profile {
            if p.games_played >= 10 {
                let (w, l, _) = self.result_counts;
                if l > w * 2 {
                    tips.push((
                        "You're losing more than two-thirds of your games - consider Adaptive mode or a lower difficulty."
                            .into(),
                        class_inaccuracy(),
                    ));
                }
            }
        }

        if tips.is_empty() {
            tips.push((
                "Keep playing and analyzing games to unlock more insights.".into(),
                hydra_accent(),
            ));
        }

        // Tips card: one header, each tip as a colored marker plus plain
        // text. The marker carries the semantic color (warning / mistake /
        // suggestion) without each tip needing its own box.
        hydra_card_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(hydra_heading("Tips", 14.0));
            ui.add_space(8.0);
            for (text, accent) in tips {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("▶").size(13.0).color(accent).strong());
                    ui.label(egui::RichText::new(text).size(12.0));
                });
                ui.add_space(6.0);
            }
        });
    }

    // ── Puzzle trainer ───────────────────────────────────────────────
}
