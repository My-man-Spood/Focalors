use super::*;

impl FocalorsApp {
    /// App-wide preferences: app theme, board colors and piece set. Future
    /// customization gets its own section here. Every choice applies and
    /// saves on click, so there is no confirm step.
    pub(crate) fn draw_settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }

        let mut open = true;
        let mut pick_ui_theme: Option<UiTheme> = None;
        let mut pick_board: Option<BoardTheme> = None;
        let mut pick_pieces: Option<PieceSet> = None;
        let mut new_volume: Option<f32> = None;
        let mut preview_volume = false;

        egui::Window::new(hydra_heading("Settings", 16.0))
            .id(egui::Id::new("settings_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            // Opens centered but stays draggable, so it can be moved off the
            // board while trying themes during a game or a review.
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.set_min_width(SETTINGS_CONTENT_W);

                ui.label(hydra_heading("APPEARANCE", 10.0).color(hydra_subtle_text()));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("App theme").size(12.5));
                    for (theme, label) in [(UiTheme::Dark, "Dark"), (UiTheme::Light, "Light")] {
                        let selected = self.ui_theme == theme;
                        if ui.add(home_nav_button(selected, label)).clicked() && !selected {
                            pick_ui_theme = Some(theme);
                        }
                    }
                });

                ui.add_space(SECTION_GAP - ui.spacing().item_spacing.y);
                ui.label(hydra_heading("BOARD", 10.0).color(hydra_subtle_text()));
                ui.horizontal_top(|ui| {
                    egui::Grid::new("settings_board_themes")
                        .num_columns(SETTINGS_SWATCH_COLS)
                        .spacing([SETTINGS_SWATCH_GAP, SETTINGS_SWATCH_GAP])
                        .show(ui, |ui| {
                            for (i, theme) in BoardTheme::ALL.into_iter().enumerate() {
                                let selected = self.board_theme == theme;
                                if board_theme_swatch(ui, theme, selected).clicked() && !selected {
                                    pick_board = Some(theme);
                                }
                                if i % SETTINGS_SWATCH_COLS == SETTINGS_SWATCH_COLS - 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    ui.add_space(SETTINGS_PREVIEW_GAP - ui.spacing().item_spacing.x);
                    self.draw_board_preview(ui, SETTINGS_PREVIEW_SIDE);
                });
                ui.label(
                    egui::RichText::new("Applies to every board: play, review, puzzles and history.")
                        .size(11.0)
                        .color(hydra_subtle_text()),
                );

                ui.add_space(SECTION_GAP - ui.spacing().item_spacing.y);
                ui.label(hydra_heading("PIECES", 10.0).color(hydra_subtle_text()));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SETTINGS_SWATCH_GAP;
                    let palette = self.board_theme.palette();
                    for set in PieceSet::ALL {
                        let selected = self.piece_set == set;
                        if let Some(textures) = self.piece_sets.get(&set)
                            && piece_set_swatch(ui, set, textures, palette, selected).clicked()
                            && !selected
                        {
                            pick_pieces = Some(set);
                        }
                    }
                });

                ui.add_space(SECTION_GAP - ui.spacing().item_spacing.y);
                ui.label(hydra_heading("SOUND", 10.0).color(hydra_subtle_text()));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Effects volume").size(12.5));
                    let mut pct = (self.sfx_volume * 100.0).round() as i32;
                    let response = ui.add(egui::Slider::new(&mut pct, 0..=100).suffix("%"));
                    if response.changed() {
                        new_volume = Some(pct as f32 / 100.0);
                    }
                    if response.drag_stopped() {
                        preview_volume = true;
                    }
                });
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        if let Some(theme) = pick_ui_theme {
            self.set_ui_theme(ctx, theme);
        }
        if let Some(theme) = pick_board {
            self.set_board_theme(theme);
        }
        if let Some(set) = pick_pieces {
            self.set_piece_set(set);
        }
        if let Some(volume) = new_volume {
            self.set_sfx_volume(volume);
        }
        if preview_volume {
            self.play_sound(crate::sound::Sfx::Move);
        }
        // One write per adjustment, not per frame: flush when no pointer
        // button is held (the slider was released) or the window goes away.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if self.sfx_volume_dirty && (!open || !pointer_down) {
            if let Some(ref db) = self.db {
                let _ = db.set_sfx_volume(self.sfx_volume as f64);
            }
            self.sfx_volume_dirty = false;
        }
        self.show_settings = open;
    }

    /// Small static board for the Settings window, so a board theme can be
    /// judged on pages that show no board. Real piece images (the downscaled
    /// set) plus the last-move highlight on two squares. White at the bottom.
    pub(crate) fn draw_board_preview(&self, ui: &mut egui::Ui, side: f32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter();
        let palette = self.board_theme.palette();
        let sq = side / 8.0;
        for visual_rank in 0..8u8 {
            // visual_rank 0 = top of the board = chess rank 8
            let chess_rank = 7 - visual_rank;
            for file in 0..8u8 {
                let sq_idx = chess_rank * 8 + file;
                let color = if SETTINGS_PREVIEW_LAST_MOVE.contains(&sq_idx) {
                    BOARD_LAST_MOVE
                } else if (file + chess_rank) % 2 == 1 {
                    palette.light
                } else {
                    palette.dark
                };
                let cell = egui::Rect::from_min_size(
                    egui::pos2(
                        rect.min.x + file as f32 * sq,
                        rect.min.y + visual_rank as f32 * sq,
                    ),
                    egui::vec2(sq, sq),
                );
                painter.rect_filled(cell, grid_cell_corner_radius(file, visual_rank, 7, 6), color);
                if let Some((piece_color, piece)) =
                    self.settings_preview_board.piece_on(Square(sq_idx))
                    && let Some(texture) = self.small_piece_texture(piece_color, piece)
                {
                    draw_piece_image(painter, texture, cell.shrink(sq * 0.05));
                }
            }
        }
    }

    /// The Custom opponent's thinking budget: a fixed time per move or a
    /// fixed depth. Opens from the Advanced button next to the profile
    /// picker, and on first picking Custom. Changes apply at once and are
    /// written to the profile row once the slider is released.
    pub(crate) fn draw_custom_opponent_window(&mut self, ctx: &egui::Context, searching: bool) {
        if !self.show_advanced_engine_settings {
            return;
        }

        let before = self.state.lock().unwrap().engine_settings.clone();
        let mut settings = before.clone();
        let mut open = true;

        egui::Window::new(hydra_heading("Custom opponent", 16.0))
            .id(egui::Id::new("custom_opponent_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.set_min_width(CUSTOM_OPPONENT_W);
                ui.set_max_width(CUSTOM_OPPONENT_W);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Only the Custom profile uses these values. The named profiles keep their own strength.",
                        )
                        .size(11.0)
                        .color(hydra_subtle_text()),
                    )
                    .wrap(),
                );

                ui.add_space(SECTION_GAP - ui.spacing().item_spacing.y);
                ui.label(hydra_heading("THINKING", 10.0).color(hydra_subtle_text()));
                ui.add_enabled_ui(!searching, |ui| {
                    ui.horizontal(|ui| {
                        for (mode, label) in [(true, "Time limit"), (false, "Fixed depth")] {
                            let selected = settings.use_time_limit == mode;
                            if ui.add(home_nav_button(selected, label)).clicked() {
                                settings.use_time_limit = mode;
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.spacing_mut().slider_width = CUSTOM_OPPONENT_W - 90.0;
                    let (t_min, t_max) = EngineSettings::THINK_TIME_MS_RANGE;
                    let (d_min, d_max) = EngineSettings::MAX_DEPTH_RANGE;
                    let hint = if settings.use_time_limit {
                        ui.label(egui::RichText::new("Think time per move").size(12.5));
                        let mut secs = settings.think_time_ms as f32 / 1000.0;
                        ui.add(
                            egui::Slider::new(
                                &mut secs,
                                (t_min as f32 / 1000.0)..=(t_max as f32 / 1000.0),
                            )
                            .suffix(" s")
                            .fixed_decimals(1),
                        );
                        settings.think_time_ms = (secs * 1000.0).round() as u64;
                        "Less time makes a quicker, weaker opponent. A move never takes longer than the clock allows."
                    } else {
                        ui.label(egui::RichText::new("Search depth per move").size(12.5));
                        let mut depth = settings.max_depth as i32;
                        ui.add(egui::Slider::new(&mut depth, (d_min as i32)..=(d_max as i32)));
                        settings.max_depth = depth as u32;
                        "Low depths play like a beginner. Above 20, a single move can take minutes."
                    };
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(hint).size(11.0).color(hydra_subtle_text()),
                        )
                        .wrap(),
                    );
                });
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        if settings != before {
            self.state.lock().unwrap().engine_settings = settings.clone();
            self.custom_engine_dirty = true;
        }
        // One write per adjustment, not per frame: flush when no pointer
        // button is held (the slider was released) or the window goes away.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if self.custom_engine_dirty && (!open || !pointer_down) {
            if let Some(ref db) = self.db {
                let _ = db.set_custom_engine(
                    settings.use_time_limit,
                    settings.think_time_ms,
                    settings.max_depth,
                );
            }
            self.custom_engine_dirty = false;
        }
        self.show_advanced_engine_settings = open;
    }
}
