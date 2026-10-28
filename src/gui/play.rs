use super::*;

impl FocalorsApp {
    /// `animate` is false for drag-drops: the piece was already dropped on
    /// its destination square, so a glide would visually replay the move.
    pub(crate) fn apply_human_move(&mut self, mv: Move, animate: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.local_game.outcome.is_some() {
            return false;
        }

        if handle_active_side_timeout(&mut state) {
            return false;
        }

        let mover = state.board.side_to_move;
        if !consume_local_turn_time(&mut state, mover) {
            return false;
        }

        let uci_str = mv.to_uci();

        if let Some(anim) = pending_animation(&state.board, mv, animate) {
            state.pending_animations.push(anim);
        }
        make_move(&mut state.board, mv);
        state.local_game.time_control.add_increment(mover);
        record_local_snapshot(&mut state, uci_str.clone());
        state.status_message = format!("You played: {uci_str}");
        update_local_game_outcome(&mut state);
        if state.local_game.outcome.is_none() {
            state.local_game.time_control.start_turn_now();
        }
        true
    }

    pub(crate) fn complete_promotion(&mut self, piece: Piece) {
        let Some(pending) = self.pending_promotion.take() else {
            return;
        };

        if let Some(mv) = pending
            .moves
            .into_iter()
            .find(|mv| mv.promotion_piece() == piece)
        {
            if self.apply_human_move(mv, true) {
                self.start_engine_search();
            }
        }
    }

    pub(crate) fn cancel_promotion(&mut self) {
        self.pending_promotion = None;
        self.selected_square = None;
        self.drag_state = None;
        self.state.lock().unwrap().status_message = "Promotion cancelled.".to_string();
    }

    /// Promotion picker: the four candidate pieces from the active set, in
    /// the mover's color, as tiles on a square of the current board. Esc or
    /// Cancel keeps the pawn where it was.
    pub(crate) fn draw_promotion_window(&mut self, ctx: &egui::Context) {
        if self.pending_promotion.is_none() {
            return;
        }

        // The mover's color follows from the move itself: White promotes on
        // the eighth rank, Black on the first. True on any board, live or
        // puzzle, without consulting engine state.
        let Some(first) = self.pending_promotion.as_ref().and_then(|p| p.moves.first()) else {
            return;
        };
        let color = if first.to_sq().0 / 8 == 7 { Color::White } else { Color::Black };
        let square_color = self.board_theme.palette().light;
        let mut chosen_piece = None;
        let mut cancel = false;

        egui::Window::new(hydra_heading("Promote to", 16.0))
            .id(egui::Id::new("promotion_window"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SETTINGS_SWATCH_GAP;
                    // Queen first, then the knight: the one underpromotion
                    // anyone picks on purpose.
                    for piece in [Piece::Queen, Piece::Knight, Piece::Rook, Piece::Bishop] {
                        let texture = self.piece_texture(color, piece);
                        if promotion_tile(ui, texture, square_color, piece_name(piece)).clicked() {
                            chosen_piece = Some(piece);
                        }
                    }
                });
                ui.add_space(6.0);
                if ui.add(secondary_button("Cancel")).clicked() {
                    cancel = true;
                }
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if let Some(piece) = chosen_piece {
            self.complete_promotion(piece);
        } else if cancel {
            self.cancel_promotion();
        }
    }

    pub(crate) fn draw_board(&mut self, ui: &mut egui::Ui, override_pos: Option<&BoardView<'_>>) {
        let interactive = override_pos.map(|v| v.interactive).unwrap_or(true);
        let (board, live_last_move) = match override_pos {
            Some(v) => (v.board.clone(), None),
            None => {
                let mut s = self.state.lock().unwrap();
                // Each drained payload drives up to two things: the visual
                // glide (only when `animate`) and the move/capture sound
                // (always — a drag-drop still needs its sound even though
                // nothing glides).
                let mut sounds_to_play: Vec<crate::sound::Sfx> = Vec::new();
                for pending in s.pending_animations.drain(..) {
                    if pending.animate {
                        self.move_animations.push(MoveAnimation {
                            pending,
                            start: Instant::now(),
                        });
                    }
                    sounds_to_play.push(move_sfx(pending.captured));
                }
                // Game-end SFX, fired exactly once per outcome no matter how
                // the game ended: checkmate and timeout already got a
                // move/capture sound above since they end on a move, but
                // resignation and an active-side timeout set the outcome
                // directly without a move. No queue needed here — just a
                // direct read of whether the outcome flipped since last
                // frame.
                if s.local_game.active {
                    if s.local_game.outcome.is_some() && !self.game_end_sound_played {
                        sounds_to_play.push(crate::sound::Sfx::GenericNotify);
                        self.game_end_sound_played = true;
                    } else if s.local_game.outcome.is_none() {
                        self.game_end_sound_played = false;
                    }
                }
                drop(s);
                for sfx in sounds_to_play {
                    self.play_sound(sfx);
                }
                let s = self.state.lock().unwrap();
                (s.board.clone(), live_last_move_squares(&s))
            }
        };
        self.move_animations
            .retain(|anim| anim.start.elapsed() < MOVE_ANIM_DURATION);

        // Snapshot of every glide in flight: (from, to, piece, eased 0..=1
        // progress), with castling rook glides flattened in. Only the live
        // board animates; replay/review boards render statically.
        let mut glides: Vec<(u8, u8, (Color, Piece), f32)> = Vec::new();
        if override_pos.is_none() {
            for anim in &self.move_animations {
                let t = (anim.start.elapsed().as_secs_f32()
                    / MOVE_ANIM_DURATION.as_secs_f32())
                    .min(1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                let p = anim.pending;
                glides.push((p.from_sq, p.to_sq, p.piece, eased));
                if let Some((rook_from, rook_to)) = p.rook {
                    glides.push((rook_from, rook_to, (p.piece.0, Piece::Rook), eased));
                }
            }
        }

        // Clear annotations when the (main) board changes. Covers move played,
        // engine reply, move-list navigation, new game — anything that shifts
        // the position invalidates user-drawn arrows/highlights.
        if interactive {
            if self.last_annotation_board_hash != Some(board.hash) {
                self.annotations.clear();
                self.right_drag_origin = None;
                self.last_annotation_board_hash = Some(board.hash);
            }
        }
        let last_move_squares: Option<(u8, u8)> = match override_pos {
            Some(v) => v
                .last_move
                .filter(|m| !m.is_null())
                .map(|m| (m.from_sq().0, m.to_sq().0)),
            None => live_last_move,
        };
        let available = ui.available_size();
        let max_height = (available.y - 18.0).max(180.0);
        let board_size = available.x.min(max_height);
        let sq_size = board_size / 8.0;
        let board_total_height = board_size + 28.0;

        // Square colors come from the board theme picked in Settings; the
        // highlights are the shared opaque set (see BOARD_SELECTED).
        let palette = active_board_theme().palette();
        let light = palette.light;
        let dark = palette.dark;
        let selected_color = BOARD_SELECTED;
        let legal_color = palette.legal;
        let last_move_color = BOARD_LAST_MOVE;

        // Compute legal moves from selected square (interactive only)
        let legal_targets: Vec<u8> = if interactive
            && let Some(from_sq) = self.selected_square
        {
            let moves = generate_legal_moves(&board);
            let mut targets = Vec::new();
            for i in 0..moves.len() {
                let mv = moves[i];
                if mv.from_sq().0 == from_sq {
                    targets.push(mv.to_sq().0);
                }
            }
            targets
        } else {
            Vec::new()
        };

        // Draw board
        let (response, painter) = ui.allocate_painter(
            egui::vec2(available.x, board_total_height),
            egui::Sense::click_and_drag(),
        );
        let board_rect = egui::Rect::from_min_size(
            egui::pos2(response.rect.center().x - board_size / 2.0, response.rect.min.y),
            egui::vec2(board_size, board_size),
        );
        let board_shell = board_rect.expand(14.0);
        let allow_input = interactive && self.local_input_enabled();
        let dragged_from_sq = if interactive {
            self.drag_state.map(|drag| drag.from_sq)
        } else {
            None
        };
        let drag_hover_sq = if interactive {
            self.drag_state.and_then(|drag| {
                board_square_from_pos(board_rect, sq_size, self.flipped, drag.pointer_pos)
            })
        } else {
            None
        };
        let selected_square = if interactive { self.selected_square } else { None };

        painter.rect_filled(
            board_shell.translate(egui::vec2(0.0, 8.0)),
            24.0,
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 62),
        );
        painter.rect_filled(board_shell, 24.0, hydra_panel_raised_fill());
        painter.rect_filled(board_shell.shrink(1.5), 22.0, hydra_panel_fill());

        for rank in 0..8u8 {
            for file in 0..8u8 {
                let display_rank = if self.flipped { rank } else { 7 - rank };
                let display_file = if self.flipped { 7 - file } else { file };
                let sq_idx = display_rank * 8 + display_file;

                let x = board_rect.min.x + file as f32 * sq_size;
                let y = board_rect.min.y + rank as f32 * sq_size;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(sq_size, sq_size),
                );

                // Square color. a1 (file 0, rank 0) must be dark — light squares
                // are the odd-parity ones, matching the standard "light square on
                // the right" / white queen on her own (light) color. This was
                // inverted (== 0), which put the white queen on a dark d1.
                let is_light = (display_rank + display_file) % 2 == 1;
                let base_color = if is_light { light } else { dark };
                let is_last_move = last_move_squares
                    .map(|(f, t)| f == sq_idx || t == sq_idx)
                    .unwrap_or(false);
                let color = if selected_square == Some(sq_idx) {
                    selected_color
                } else if drag_hover_sq == Some(sq_idx) {
                    BOARD_DRAG_HOVER
                } else if legal_targets.contains(&sq_idx) {
                    legal_color
                } else if is_last_move {
                    last_move_color
                } else {
                    base_color
                };

                painter.rect_filled(rect, 0.0, color);

                // User-drawn square highlight (right-click annotation)
                if interactive {
                    for annot in &self.annotations {
                        if let Annotation::Highlight { sq: h_sq, color: h_color } = *annot
                            && h_sq == sq_idx
                        {
                            painter.rect_filled(rect, 0.0, h_color.fill());
                        }
                    }
                }

                // Draw piece (hidden while a glide is still en route to it)
                if let Some((piece_color, piece)) = board.piece_on(Square(sq_idx)) {
                    if dragged_from_sq == Some(sq_idx)
                        || glides.iter().any(|g| g.1 == sq_idx)
                    {
                        continue;
                    }

                    if let Some(tex) = self.piece_texture(piece_color, piece) {
                        let padding = sq_size * 0.05;
                        let img_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x + padding, rect.min.y + padding),
                            egui::vec2(sq_size - 2.0 * padding, sq_size - 2.0 * padding),
                        );
                        draw_piece_image(&painter, tex, img_rect);
                    }
                }

                // Legal move dot
                if legal_targets.contains(&sq_idx) && board.piece_on(Square(sq_idx)).is_none() {
                    painter.circle_filled(
                        rect.center(),
                        sq_size * 0.15,
                        BOARD_LEGAL_DOT,
                    );
                }
            }
        }

        // Pieces gliding between squares, drawn on top of the static board.
        for (from_sq, to_sq, piece, t) in &glides {
            if let Some(texture) = self.piece_texture(piece.0, piece.1) {
                let from_c =
                    board_square_rect(board_rect, sq_size, self.flipped, *from_sq).center();
                let to_c = board_square_rect(board_rect, sq_size, self.flipped, *to_sq).center();
                let center = from_c + (to_c - from_c) * *t;
                let padding = sq_size * 0.05;
                let side = sq_size - 2.0 * padding;
                let img_rect = egui::Rect::from_center_size(center, egui::vec2(side, side));
                draw_piece_image(&painter, texture, img_rect);
            }
        }
        if !glides.is_empty() {
            // Animate at full frame rate for the glide's ~160ms, instead of
            // the lazy 100-200ms idle repaint cadence.
            ui.ctx().request_repaint();
        }

        // File labels
        let label_y = board_rect.max.y + 2.0;
        for file in 0..8u8 {
            let display_file = if self.flipped { 7 - file } else { file };
            let x = board_rect.min.x + file as f32 * sq_size + sq_size / 2.0;
            painter.text(
                egui::pos2(x, label_y),
                egui::Align2::CENTER_TOP,
                (b'a' + display_file) as char,
                egui::FontId::proportional(12.0),
                egui::Color32::GRAY,
            );
        }

        // Rank labels
        for rank in 0..8u8 {
            let display_rank = if self.flipped { rank } else { 7 - rank };
            let y = board_rect.min.y + rank as f32 * sq_size + sq_size / 2.0;
            painter.text(
                egui::pos2(board_rect.min.x - 12.0, y),
                egui::Align2::CENTER_CENTER,
                (b'1' + display_rank) as char,
                egui::FontId::proportional(12.0),
                egui::Color32::GRAY,
            );
        }

        // User-drawn arrows (right-click annotations) — rendered last so they
        // sit on top of pieces. Highlights were drawn earlier under the pieces.
        if interactive {
            for annot in &self.annotations {
                if let Annotation::Arrow { from, to, color } = *annot {
                    let from_pos = board_square_center(board_rect, sq_size, self.flipped, from);
                    let to_pos   = board_square_center(board_rect, sq_size, self.flipped, to);
                    draw_annotation_arrow(&painter, from_pos, to_pos, color.arrow(), sq_size);
                }
            }
        }

        // Game Review overlay arrows: the played move (classification color,
        // drawn only when it differs from best) and the engine's best move
        // (green). Rendered last so they sit above the pieces. flip-aware via
        // board_square_center, so they always match the board's orientation.
        if let Some(rv) = override_pos.and_then(|v| v.review) {
            let alpha = 205u8;
            if let Some((pf, pt)) = rv.played
                && rv.best != Some((pf, pt))
            {
                let c = rv.played_color;
                draw_annotation_arrow(
                    &painter,
                    board_square_center(board_rect, sq_size, self.flipped, pf),
                    board_square_center(board_rect, sq_size, self.flipped, pt),
                    egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha),
                    sq_size,
                );
            }
            if let Some((bf, bt)) = rv.best {
                draw_annotation_arrow(
                    &painter,
                    board_square_center(board_rect, sq_size, self.flipped, bf),
                    board_square_center(board_rect, sq_size, self.flipped, bt),
                    egui::Color32::from_rgba_unmultiplied(94, 176, 102, alpha),
                    sq_size,
                );
            }

            // Classification badge on the played move's destination square, in
            // the chess.com corner style. Only moves whose symbol is non-empty
            // are badged, which after MoveClass::symbol() means Inaccuracy /
            // Mistake / Blunder / Brilliant (Best/Good/Forced return ""), so it
            // stays uncluttered. Symbols are ASCII (?! ? ?? !!) — never tofu.
            if let Some((_, pt)) = rv.played {
                let sym = rv.played_class.symbol();
                if !sym.is_empty() {
                    let center = board_square_center(board_rect, sq_size, self.flipped, pt);
                    let badge_center = center + egui::vec2(sq_size * 0.32, -sq_size * 0.32);
                    let ink = egui::Color32::from_rgb(24, 20, 16);
                    painter.circle(
                        badge_center,
                        sq_size * 0.19,
                        classification_color(rv.played_class),
                        egui::Stroke::new(1.5_f32, ink),
                    );
                    painter.text(
                        badge_center,
                        egui::Align2::CENTER_CENTER,
                        sym,
                        egui::FontId::proportional(sq_size * 0.22),
                        ink,
                    );
                }
            }
        }

        // Right-click annotation handling — press records origin square,
        // release resolves into a highlight (no drag) or arrow (drag), with
        // modifier keys selecting the color.
        if interactive {
            let secondary_pressed = ui.ctx().input(|i| i.pointer.secondary_pressed());
            let secondary_released = ui.ctx().input(|i| i.pointer.secondary_released());
            let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
            let modifiers = ui.ctx().input(|i| i.modifiers);

            if secondary_pressed
                && let Some(pos) = pointer_pos
                && let Some(sq) = board_square_from_pos(board_rect, sq_size, self.flipped, pos)
            {
                self.right_drag_origin = Some(sq);
            }

            if secondary_released
                && let Some(from_sq) = self.right_drag_origin.take()
                && let Some(pos) = pointer_pos
                && let Some(to_sq) = board_square_from_pos(board_rect, sq_size, self.flipped, pos)
            {
                let color = AnnotationColor::from_modifiers(modifiers);
                let new_annot = if from_sq == to_sq {
                    Annotation::Highlight { sq: from_sq, color }
                } else {
                    Annotation::Arrow { from: from_sq, to: to_sq, color }
                };
                self.toggle_annotation(new_annot);
            }
        }

        if response.drag_started() && allow_input && self.pending_promotion.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(from_sq) = board_square_from_pos(board_rect, sq_size, self.flipped, pos)
                {
                    if let Some((piece_color, piece)) = board.piece_on(Square(from_sq)) {
                        if piece_color == board.side_to_move {
                            self.drag_state = Some(DragState {
                                from_sq,
                                piece_color,
                                piece,
                                pointer_pos: pos,
                            });
                            self.selected_square = Some(from_sq);
                        }
                    }
                }
            }
        }

        if interactive
            && let Some(drag_state) = &mut self.drag_state
        {
            if let Some(pos) = response.interact_pointer_pos() {
                drag_state.pointer_pos = pos;
            }
        }

        // A left-click anywhere on the board also clears annotations (matches
        // lichess/chess.com convention — you click to commit to the plan, the
        // arrows you drew while thinking go away). Fires even if `allow_input`
        // is false so reviewing the board still respects the clear.
        if interactive && response.clicked() {
            self.annotations.clear();
        }

        // Handle clicks
        if response.clicked() && allow_input && self.pending_promotion.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(sq_idx) = board_square_from_pos(board_rect, sq_size, self.flipped, pos)
                {
                    if let Some(from_sq) = self.selected_square {
                        // Try to make a move (click-moves glide)
                        let success = self.try_make_move(from_sq, sq_idx, true);
                        self.selected_square = None;
                        if success {
                            // Engine responds
                            self.start_engine_search();
                        }
                    } else {
                        // Select a piece
                        let b = self.state.lock().unwrap().board.clone();
                        if let Some((color, _)) = b.piece_on(Square(sq_idx)) {
                            if color == b.side_to_move {
                                self.selected_square = Some(sq_idx);
                            }
                        }
                    }
                }
            }
        }

        let pointer_down = ui.ctx().input(|input| input.pointer.primary_down());
        if interactive && let Some(drag_state) = self.drag_state {
            if let Some(texture) =
                self.piece_texture(drag_state.piece_color, drag_state.piece)
            {
                let piece_size = sq_size * 0.94;
                let piece_rect = egui::Rect::from_center_size(
                    drag_state.pointer_pos,
                    egui::vec2(piece_size, piece_size),
                );
                draw_piece_image(&painter, texture, piece_rect);
            }

            if !pointer_down {
                self.drag_state = None;
                self.selected_square = None;

                if allow_input {
                    if let Some(target_sq) = board_square_from_pos(
                        board_rect,
                        sq_size,
                        self.flipped,
                        drag_state.pointer_pos,
                    ) {
                        let success = self.try_make_move(drag_state.from_sq, target_sq, false);
                        if success {
                            self.start_engine_search();
                        }
                    }
                }
            }
        }

        if interactive {
            self.draw_promotion_window(ui.ctx());
        }
    }

    pub(crate) fn try_make_move(&mut self, from: u8, to: u8, animate: bool) -> bool {
        let state = self.state.lock().unwrap();
        let moves = generate_legal_moves(&state.board);
        let in_puzzle = self.puzzle_trainer.is_some();
        drop(state);

        let mut promotion_moves = Vec::new();

        for i in 0..moves.len() {
            let mv = moves[i];
            if mv.from_sq().0 == from && mv.to_sq().0 == to {
                if matches!(mv.flag(), MoveFlag::Promotion) {
                    promotion_moves.push(mv);
                    continue;
                }

                if in_puzzle {
                    self.handle_puzzle_move(mv, animate);
                    return true;
                }
                return self.apply_human_move(mv, animate);
            }
        }

        if !promotion_moves.is_empty() {
            self.pending_promotion = Some(PendingPromotion {
                moves: promotion_moves,
            });
            self.state.lock().unwrap().status_message = "Choose a promotion piece.".to_string();
        }

        false
    }

    pub(crate) fn start_engine_search(&self) {
        self.start_engine_search_inner(false);
    }

    pub(crate) fn start_engine_search_inner(&self, force: bool) {
        let state = self.state.clone();
        let local_request = {
            let mut s = state.lock().unwrap();

            if s.search_info.searching {
                return;
            }

            if !force {
                if !s.local_game.active
                    || s.local_game.outcome.is_some()
                    || s.board.side_to_move == s.local_game.human_color
                {
                    return;
                }
            }

            if handle_active_side_timeout(&mut s) {
                return;
            }

            let moves = generate_legal_moves(&s.board);
            if moves.is_empty() {
                return;
            }

            let config = crate::strength::StrengthConfig::from_level(s.local_game.numeric_level);
            local_engine_search_request(&s, &config)
        };

        thread::spawn(move || {
            // RAII guard: if this worker exits via panic, search_info.searching
            // would stay `true` forever and the GUI would show the "thinking"
            // state with no way to recover except restart. On unwind the guard
            // clears the flag and surfaces a status message — but ONLY if we
            // are still the active search (generation match). If a newer
            // search has taken over (e.g. invalidate_local_search bumped the
            // generation and started another search), our state belongs to
            // that newer search and we leave it alone. On the success path
            // the worker explicitly sets searching=false at the end, so the
            // guard sees a cleared flag and does nothing.
            struct SearchGuard {
                state: Arc<Mutex<SharedState>>,
                our_generation: Option<u64>,
            }
            impl Drop for SearchGuard {
                fn drop(&mut self) {
                    let mut s = match self.state.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let still_current = match self.our_generation {
                        Some(our) => s.local_search_generation == our,
                        None => true,
                    };
                    if !still_current { return; }
                    if s.search_info.searching {
                        s.search_info.searching = false;
                        s.status_message =
                            "Search failed unexpectedly. Make your move or restart.".to_string();
                        eprintln!(
                            "focalors: search worker exited unexpectedly; \
                             UI reset"
                        );
                    }
                }
            }
            let _search_guard = SearchGuard {
                state: state.clone(),
                our_generation: local_request.as_ref().map(|r| r.generation),
            };

            let (board, settings) = {
                let mut s = state.lock().unwrap();
                // A superseded worker must not touch state at ALL. The stale
                // exit paths (and SearchGuard) deliberately skip cleanup on a
                // generation mismatch, assuming the newer worker still runs;
                // now that book replies finish in microseconds, a newer worker
                // can complete entirely before a delayed stale worker's first
                // lock, and an unguarded write here would wedge
                // searching=true forever in an active game.
                if let Some(ref request) = local_request
                    && s.local_search_generation != request.generation
                {
                    return;
                }
                s.search_info.searching = true;
                s.status_message = if force {
                    "Engine thinking...".to_string()
                } else {
                    "Focalors thinking...".to_string()
                };
                (s.board.clone(), s.engine_settings.clone())
            };

            // Opening book: local games answer known opening positions with
            // an instant, weighted-random book reply instead of a full
            // search. Book depth scales with difficulty (a beginner "knows"
            // one move of theory, Master the whole book). Off book, past the
            // level's budget, or on any anomaly this is None and the normal
            // search below runs, so the book can only ever skip work, never
            // change it. UCI mode and selfmatch never get here.
            let book_move = local_request
                .as_ref()
                .and_then(|req| crate::book::pick_book_move(&board, req.strength_config.level));

            // A book reply is computed in microseconds; pause briefly (never
            // while holding the state lock) so the coach appears to consider
            // the position instead of teleporting a piece. The pause runs on
            // the engine's clock like a real short think, and shrinks with
            // the time allocation when the engine is low. A stale worker
            // sleeping here is harmless: the generation checks at entry and
            // apply already cover it.
            if book_move.is_some()
                && let Some(ref request) = local_request
            {
                let delay_ms =
                    crate::book::humanized_delay_ms().min(request.hard_time_ms / 4);
                thread::sleep(Duration::from_millis(delay_ms));
            }

            let (final_move, search_result) = if let Some(book_mv) = book_move {
                (book_mv, None)
            } else {
                // Build position history for repetition detection
                let position_hashes: Vec<u64> = {
                    let s = state.lock().unwrap();
                    s.local_history.iter().map(|snap| snap.board.hash).collect()
                };

                // Use persistent searcher for TT reuse across moves
                let searcher_arc = state.lock().unwrap().persistent_searcher.clone();
                let mut searcher = searcher_arc.lock().unwrap();
                searcher.set_position_history(position_hashes.clone());

                let result = if let Some(ref request) = local_request {
                    if request.depth_cap.is_none() {
                        // Master / time-bound Custom: Lazy SMP across all cores.
                        // The TT is shared with the persistent searcher so cross-
                        // move TT reuse still works.
                        let tt = searcher.tt.clone();
                        let use_nnue = searcher.use_nnue;
                        let n_threads = std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(1);
                        crate::search::search_lazy_smp(
                            tt,
                            &board,
                            use_nnue,
                            n_threads,
                            request.soft_time_ms,
                            request.hard_time_ms,
                            request.depth_cap,
                            position_hashes,
                        )
                    } else {
                        // Beginner / Club / Tournament / depth-bound Custom:
                        // single-thread is more efficient for shallow depth caps.
                        searcher.search_with_time_management_capped(
                            &board,
                            request.soft_time_ms,
                            request.hard_time_ms,
                            request.depth_cap,
                        )
                    }
                } else if settings.use_time_limit {
                    searcher.search_timed(&board, settings.think_time_ms)
                } else {
                    searcher.search(&board, settings.max_depth)
                };

                // Apply weighted move selection for difficulty levels that use it
                let final_move = if let Some(ref request) = local_request {
                    crate::strength::select_move(
                        &mut searcher,
                        &board,
                        result.best_move,
                        result.score,
                        result.depth,
                        &request.strength_config,
                    )
                } else {
                    result.best_move
                };
                (final_move, Some(result))
            };

            let mut s = state.lock().unwrap();
            if let Some(ref request) = local_request {
                if s.local_search_generation != request.generation {
                    return;
                }
            }
            // Book moves leave the previous search_info (and thus the eval
            // bar) untouched; there is no new search data to show.
            if let Some(ref result) = search_result {
                s.search_info.depth = result.depth;
                s.search_info.score = result.score;
                s.search_info.nodes = result.nodes;
                s.search_info.best_move = final_move.to_uci();
            }
            s.search_info.searching = false;

            if s.local_game.active && s.local_game.outcome.is_some() {
                return;
            }

            if !final_move.is_null() {
                if s.local_game.active {
                    let mover = s.board.side_to_move;
                    if !consume_local_turn_time(&mut s, mover) {
                        return;
                    }
                    s.local_game.time_control.add_increment(mover);
                }

                let uci_str = final_move.to_uci();
                if let Some(anim) = pending_animation(&s.board, final_move, true) {
                    s.pending_animations.push(anim);
                }
                make_move(&mut s.board, final_move);
                if s.local_game.active {
                    record_local_snapshot(&mut s, uci_str.clone());
                    update_local_game_outcome(&mut s);
                    if s.local_game.outcome.is_none() {
                        s.local_game.time_control.start_turn_now();
                        s.status_message = format!("Focalors played: {uci_str}. Your move.");
                    }
                } else {
                    s.move_history.push(uci_str.clone());
                    s.status_message = if let Some(ref result) = search_result {
                        format!(
                            "Engine played: {} (depth {}, {} cp, {} nodes)",
                            uci_str, result.depth, result.score, result.nodes
                        )
                    } else {
                        format!("Engine played: {uci_str}")
                    };
                }
            }
        });
    }

    // ── Compact bottom status bar (clocks + status + eval) ─────────────

    pub(crate) fn draw_play_status_bar(&self, ui: &mut egui::Ui) {
        let state = self.state.lock().unwrap();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&state.status_message)
                        .size(12.0)
                        .color(hydra_text()),
                )
                .truncate(),
            );

            if state.search_info.depth > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // search_info.score comes from the engine's own search, so it
                    // is signed from the engine's perspective. The engine is always
                    // your opponent here, so negating it gives a "you-positive"
                    // reading: + means you are winning, - means you are losing.
                    let score = -state.search_info.score;
                    let score_text = format_eval(score);

                    if !state.search_info.best_move.is_empty() {
                        ui.label(
                            egui::RichText::new(&state.search_info.best_move)
                                .size(11.0)
                                .strong()
                                .monospace()
                                .color(hydra_success()),
                        );
                        ui.label(
                            egui::RichText::new("best")
                                .size(10.0)
                                .color(hydra_subtle_text()),
                        );
                        ui.add_space(10.0);
                    }

                    ui.label(
                        egui::RichText::new(score_text)
                            .size(12.0)
                            .strong()
                            .monospace()
                            .color(hydra_accent()),
                    );
                    ui.label(
                        egui::RichText::new("eval")
                            .size(10.0)
                            .color(hydra_subtle_text()),
                    );
                    ui.add_space(10.0);

                    ui.label(
                        egui::RichText::new(format!(
                            "d{} · {}",
                            state.search_info.depth,
                            format_nodes(state.search_info.nodes)
                        ))
                        .size(11.0)
                        .monospace()
                        .color(hydra_subtle_text()),
                    );
                });
            } else if state.search_info.searching {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Focalors thinking…")
                            .size(11.0)
                            .color(hydra_warning()),
                    );
                });
            }
        });
    }

    // ── Controls panel ─────────────────────────────────────────────────

    pub(crate) fn draw_controls(&mut self, ui: &mut egui::Ui) {
        let local_active = self.state.lock().unwrap().local_game.active;
        if local_active {
            self.draw_local_game_controls(ui);
        }
    }

    /// Right-hand panel of the live game: a clocks card mirroring the board
    /// orientation, the Game Session card (summary, quick actions, history,
    /// status or outcome) and the optional eval breakdown card.
    pub(crate) fn draw_local_game_controls(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("local_controls").show(ui, |ui| {
            let (
                searching,
                local_game,
                side_to_move,
                history_cursor,
                history_len,
                history_choices,
                history_paused,
                white_ms,
                black_ms,
            ) = {
                let state = self.state.lock().unwrap();
                let history_choices = state
                    .local_history
                    .iter()
                    .enumerate()
                    .map(|(index, snapshot)| {
                        (index, local_history_label(index, snapshot.move_uci.as_deref()))
                    })
                    .collect::<Vec<_>>();
                let running_side = if state.local_game.outcome.is_none() {
                    Some(state.board.side_to_move)
                } else {
                    None
                };
                let tc = &state.local_game.time_control;
                (
                    state.search_info.searching,
                    state.local_game.clone(),
                    state.board.side_to_move,
                    state.local_history_cursor,
                    state.local_history.len(),
                    history_choices,
                    is_reviewing_history(&state) || state.local_game.time_control.active_since.is_none(),
                    tc.displayed_remaining_ms(Color::White, running_side),
                    tc.displayed_remaining_ms(Color::Black, running_side),
                )
            };

            let mut navigate_to = None;
            let mut resume = false;
            let mut abort = false;
            let mut want_review = false;

            // Keyboard navigation through the move history, mirroring the
            // Game Review overlay's arrow/Home/End bindings.
            let (key_left, key_right, key_home, key_end) = ui.ctx().input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.key_pressed(egui::Key::Home),
                    i.key_pressed(egui::Key::End),
                )
            });
            if key_left && history_cursor > 0 {
                navigate_to = Some(history_cursor - 1);
            }
            if key_right && history_cursor + 1 < history_len {
                navigate_to = Some(history_cursor + 1);
            }
            if key_home && history_len > 0 {
                navigate_to = Some(0);
            }
            if key_end && history_len > 0 {
                navigate_to = Some(history_len - 1);
            }

            // Index 0 is the start-position seed; anything past it is a move.
            let has_moves = history_len > 1;
            let human = local_game.human_color;
            let game_over = local_game.outcome.is_some();

            // ── Clocks ─────────────────────────────────────────────────
            // The side shown at the top of the board comes first, so the
            // card mirrors the board orientation (and follows Flip Board).
            let player_name = self
                .profile
                .as_ref()
                .map(|p| p.name.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "You".to_string());
            let engine_sub = format!(
                "{} · {}",
                color_name(human.flip()),
                local_game.difficulty.label()
            );
            let human_sub = color_name(human).to_string();
            let (human_ms, engine_ms) = match human {
                Color::White => (white_ms, black_ms),
                Color::Black => (black_ms, white_ms),
            };
            let human_active = !game_over && side_to_move == human;
            let engine_active = !game_over && side_to_move != human;
            let human_row = (player_name.as_str(), human_sub.as_str(), human_ms, human_active);
            let engine_row = ("Focalors", engine_sub.as_str(), engine_ms, engine_active);
            let bottom_color = if self.flipped { Color::Black } else { Color::White };
            let (top, bottom) = if human == bottom_color {
                (engine_row, human_row)
            } else {
                (human_row, engine_row)
            };
            hydra_card_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                draw_clock_row(ui, top.0, top.1, top.2, top.3);
                ui.add_space(4.0);
                draw_clock_row(ui, bottom.0, bottom.1, bottom.2, bottom.3);
            });

            ui.add_space(CARD_GAP);

            // ── Game Session ───────────────────────────────────────────
            hydra_card_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(hydra_heading("Game Session", 15.0));
                ui.add_space(8.0);

                egui::Grid::new("local_session_summary")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(hydra_heading("You", 10.0).color(hydra_subtle_text()));
                        ui.label(
                            egui::RichText::new(color_name(human))
                                .size(12.0)
                                .color(hydra_text()),
                        );
                        ui.end_row();

                        ui.label(hydra_heading("Profile", 10.0).color(hydra_subtle_text()));
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                local_game.time_control.label,
                                local_game.difficulty.label()
                            ))
                            .size(12.0)
                            .color(hydra_text()),
                        );
                        ui.end_row();
                    });

                if history_paused && !game_over {
                    ui.add_space(8.0);
                    hydra_callout_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("History view active. Clocks stay paused until you resume.")
                                .size(11.0)
                                .color(hydra_warning()),
                        );
                    });
                }

                // Quick actions: a two-column button grid so the buttons
                // always fill the card instead of wrapping unevenly.
                ui.add_space(SECTION_GAP);
                ui.label(hydra_heading("QUICK ACTIONS", 10.0).color(hydra_subtle_text()));
                ui.add_space(6.0);
                let eval_label = if self.show_eval_panel { "Hide Eval" } else { "Show Eval" };
                ui.columns(2, |cols| {
                    let w = cols[0].available_width();
                    if cols[0].add_sized([w, 32.0], secondary_button("Flip Board")).clicked() {
                        self.flipped = !self.flipped;
                    }
                    let w = cols[1].available_width();
                    if cols[1].add_sized([w, 32.0], secondary_button(eval_label)).clicked() {
                        self.show_eval_panel = !self.show_eval_panel;
                    }
                });
                if !game_over {
                    ui.add_space(6.0);
                    ui.columns(2, |cols| {
                        // Resign and End Game both take two clicks: the first
                        // arms the button for a few seconds and relabels it,
                        // the second goes through. Like resigning on lichess.
                        let now = Instant::now();
                        let resign_armed = self
                            .resign_confirm_until
                            .is_some_and(|until| now < until);
                        if !resign_armed {
                            self.resign_confirm_until = None;
                        }
                        let resign_label = if resign_armed { "Confirm resign" } else { "Resign" };
                        let resign_clicked = cols[0]
                            .add_enabled_ui(!searching, |ui| {
                                let w = ui.available_width();
                                ui.add_sized([w, 32.0], danger_button(resign_label)).clicked()
                            })
                            .inner;
                        if resign_clicked {
                            if resign_armed {
                                self.resign_confirm_until = None;
                                let mut s = self.state.lock().unwrap();
                                self.pending_promotion = None;
                                self.selected_square = None;
                                self.drag_state = None;
                                set_local_game_outcome(
                                    &mut s,
                                    GameOutcome::Resignation(human.flip()),
                                );
                            } else {
                                self.resign_confirm_until = Some(now + DANGER_CONFIRM_WINDOW);
                            }
                        }
                        // End Game discards the game: not saved, not rated.
                        let end_armed = self
                            .end_game_confirm_until
                            .is_some_and(|until| now < until);
                        if !end_armed {
                            self.end_game_confirm_until = None;
                        }
                        let end_label = if end_armed { "Confirm end" } else { "End Game" };
                        let w = cols[1].available_width();
                        if cols[1].add_sized([w, 32.0], danger_button(end_label)).clicked() {
                            if end_armed {
                                self.end_game_confirm_until = None;
                                abort = true;
                            } else {
                                self.end_game_confirm_until = Some(now + DANGER_CONFIRM_WINDOW);
                            }
                        }
                    });
                    let hint = if self.resign_confirm_until.is_some() {
                        Some("Counts as a loss and is rated. Click again to confirm.")
                    } else if self.end_game_confirm_until.is_some() {
                        Some("Ends the game without saving or rating it. Click again to confirm.")
                    } else {
                        None
                    };
                    if let Some(hint) = hint {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(hint).size(11.0).color(hydra_danger()),
                            )
                            .wrap(),
                        );
                        // Keep repainting so the buttons revert on time even
                        // while nothing else moves.
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    }
                }

                ui.add_space(SECTION_GAP);
                ui.label(hydra_heading("HISTORY", 10.0).color(hydra_subtle_text()));
                ui.add_space(6.0);
                ui.columns(3, |cols| {
                    let w = cols[0].available_width();
                    if cols[0]
                        .add_enabled(
                            history_cursor > 0,
                            secondary_button("< Prev").min_size(egui::vec2(w, 30.0)),
                        )
                        .clicked()
                    {
                        navigate_to = Some(history_cursor - 1);
                    }
                    let w = cols[1].available_width();
                    if cols[1]
                        .add_enabled(
                            history_cursor + 1 < history_len,
                            secondary_button("Next >").min_size(egui::vec2(w, 30.0)),
                        )
                        .clicked()
                    {
                        navigate_to = Some(history_cursor + 1);
                    }
                    let w = cols[2].available_width();
                    egui::containers::menu::MenuButton::from_button(
                        secondary_button("Jump").min_size(egui::vec2(w, 30.0)),
                    )
                    .ui(&mut cols[2], |ui| {
                        for (index, label) in &history_choices {
                            let selected = *index == history_cursor;
                            if ui.selectable_label(selected, label).clicked() {
                                navigate_to = Some(*index);
                                ui.close();
                            }
                        }
                    });
                });

                if !game_over {
                    ui.add_space(6.0);
                    let resume_enabled = !searching && history_paused;
                    let resume_clicked = ui
                        .add_enabled_ui(resume_enabled, |ui| {
                            let w = ui.available_width();
                            ui.add_sized([w, 32.0], primary_button("Resume from Here")).clicked()
                        })
                        .inner;
                    if resume_clicked {
                        resume = true;
                    }
                }

                if let Some((_, current_label)) = history_choices.get(history_cursor) {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(format!("Current position: {current_label}"))
                            .size(11.0)
                            .color(hydra_subtle_text()),
                    );
                }

                ui.add_space(SECTION_GAP);
                if let Some(outcome) = local_game.outcome {
                    let (banner, color) = local_outcome_banner(outcome, human);
                    ui.label(hydra_heading(banner, 14.0).color(color));
                    if let Some((old, new)) = self.last_rating_change {
                        let delta = new - old;
                        let sign = if delta >= 0 { "+" } else { "" };
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(format!("Rating {old} -> {new} ({sign}{delta})"))
                                .size(12.0)
                                .color(hydra_subtle_text()),
                        );
                    }
                    ui.add_space(10.0);
                    if has_moves {
                        let w = ui.available_width();
                        if ui.add_sized([w, 34.0], primary_button("Review this game")).clicked() {
                            want_review = true;
                        }
                        ui.add_space(6.0);
                    }
                    let w = ui.available_width();
                    if ui.add_sized([w, 32.0], secondary_button("Back to Home")).clicked() {
                        abort = true;
                    }
                    if has_moves {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Review opens Game Review and runs the analysis.")
                                .size(11.0)
                                .color(hydra_subtle_text()),
                        );
                    }
                } else {
                    let (status_text, status_color) = if searching {
                        ("Focalors is thinking...", hydra_warning())
                    } else if history_paused {
                        ("Reviewing the game history.", hydra_subtle_text())
                    } else if side_to_move == human {
                        ("Your turn.", hydra_text())
                    } else {
                        ("Focalors to move.", hydra_subtle_text())
                    };
                    ui.label(hydra_heading(status_text, 13.0).color(status_color));
                }
            });

            if self.show_eval_panel {
                ui.add_space(CARD_GAP);
                self.draw_eval_panel(ui);
            }

            if want_review {
                self.review_finished_game();
            } else if abort {
                self.abort_local_game();
            }
            if let Some(target_index) = navigate_to {
                self.navigate_local_history(target_index);
            }
            if resume {
                self.resume_local_game();
            }
        });
    }
}
