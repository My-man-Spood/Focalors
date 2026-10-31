use super::*;

/// Board color preset, picked in Settings. Only the two square colors differ
/// between presets: the selection and last-move highlights are the same gold
/// on every board, and the legal-move tint is derived from the squares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BoardTheme {
    Walnut,
    Maple,
    Forest,
    Glacier,
    Hydro,
    Indigo,
    Slate,
    Rosewood,
}

impl BoardTheme {
    /// Display order of the Settings swatch grid. Must follow the
    /// declaration order, since `index` is the discriminant.
    pub(crate) const ALL: [BoardTheme; 8] = [
        BoardTheme::Walnut,
        BoardTheme::Maple,
        BoardTheme::Forest,
        BoardTheme::Glacier,
        BoardTheme::Hydro,
        BoardTheme::Indigo,
        BoardTheme::Slate,
        BoardTheme::Rosewood,
    ];

    pub(crate) fn index(self) -> u8 {
        self as u8
    }

    /// Inverse of `index`. Out-of-range values fall back to Walnut.
    pub(crate) fn from_index(index: u8) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(BoardTheme::Walnut)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            BoardTheme::Walnut => "Walnut",
            BoardTheme::Maple => "Maple",
            BoardTheme::Forest => "Forest",
            BoardTheme::Glacier => "Glacier",
            BoardTheme::Hydro => "Hydro",
            BoardTheme::Indigo => "Indigo",
            BoardTheme::Slate => "Slate",
            BoardTheme::Rosewood => "Rosewood",
        }
    }

    /// String form persisted to settings.toml.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BoardTheme::Walnut => "walnut",
            BoardTheme::Maple => "maple",
            BoardTheme::Forest => "forest",
            BoardTheme::Glacier => "glacier",
            BoardTheme::Hydro => "hydro",
            BoardTheme::Indigo => "indigo",
            BoardTheme::Slate => "slate",
            BoardTheme::Rosewood => "rosewood",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to Walnut so a
    /// corrupted row, or a preset written by a newer version, doesn't
    /// break startup.
    pub(crate) fn from_str(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.as_str() == s)
            .unwrap_or(BoardTheme::Walnut)
    }

    /// (light square, dark square).
    pub(crate) fn squares(self) -> (egui::Color32, egui::Color32) {
        let rgb = egui::Color32::from_rgb;
        match self {
            BoardTheme::Walnut => (rgb(227, 218, 201), rgb(120, 99, 81)),
            BoardTheme::Maple => (rgb(240, 217, 181), rgb(181, 136, 99)),
            BoardTheme::Forest => (rgb(235, 236, 208), rgb(115, 149, 82)),
            BoardTheme::Glacier => (rgb(222, 227, 230), rgb(140, 162, 173)),
            BoardTheme::Hydro => (rgb(221, 238, 236), rgb(72, 138, 152)),
            BoardTheme::Indigo => (rgb(226, 230, 246), rgb(108, 124, 192)),
            BoardTheme::Slate => (rgb(216, 219, 224), rgb(118, 126, 138)),
            BoardTheme::Rosewood => (rgb(236, 216, 200), rgb(152, 88, 74)),
        }
    }

    pub(crate) fn palette(self) -> BoardPalette {
        let (light, dark) = self.squares();
        // Squares the selected piece can move to get one flat tone between
        // the two square colors, a little nearer the dark one. Walnut keeps
        // the exact taupe it always had rather than the computed value.
        let legal = match self {
            BoardTheme::Walnut => egui::Color32::from_rgb(168, 149, 126),
            _ => mix_rgb(light, dark, 0.55),
        };
        BoardPalette { light, dark, legal }
    }
}

/// The colors a board theme contributes to `draw_board`, the History
/// thumbnails and the Settings preview.
#[derive(Clone, Copy)]
pub(crate) struct BoardPalette {
    pub(crate) light: egui::Color32,
    pub(crate) dark: egui::Color32,
    /// Fill for squares the selected piece can move to.
    pub(crate) legal: egui::Color32,
}

pub(crate) fn active_board_theme() -> BoardTheme {
    BoardTheme::from_index(ACTIVE_BOARD_THEME.load(Ordering::Relaxed))
}

/// Linear blend of two opaque colors: `t` = 0 gives `a`, `t` = 1 gives `b`.
fn mix_rgb(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

/// Piece graphics set, picked in Settings. Every set ships the same twelve
/// 128px PNGs (rasterized from the SVGs beside them in `assets/pieces/`),
/// so the board code draws any of them the same way.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum PieceSet {
    Cburnett,
    Rhosgfx,
}

impl PieceSet {
    /// Display order of the Settings swatches.
    pub(crate) const ALL: [PieceSet; 2] = [PieceSet::Cburnett, PieceSet::Rhosgfx];

    pub(crate) fn label(self) -> &'static str {
        match self {
            PieceSet::Cburnett => "Cburnett",
            PieceSet::Rhosgfx => "RhosGFX",
        }
    }

    /// String form persisted to settings.toml. Doubles as the set's
    /// directory name under `assets/pieces/`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            PieceSet::Cburnett => "cburnett",
            PieceSet::Rhosgfx => "rhosgfx",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to Cburnett, the set
    /// every board showed before sets became a choice.
    pub(crate) fn from_str(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|set| set.as_str() == s)
            .unwrap_or(PieceSet::Cburnett)
    }

    /// The set's twelve embedded images: (side, piece, texture name, PNG
    /// bytes), white first, king to pawn.
    pub(crate) fn images(self) -> &'static [(Color, Piece, &'static str, &'static [u8]); 12] {
        match self {
            PieceSet::Cburnett => &CBURNETT_IMAGES,
            PieceSet::Rhosgfx => &RHOSGFX_IMAGES,
        }
    }
}

pub(crate) fn uci_arrow_squares(uci: &str) -> Option<(u8, u8)> {
    if uci.len() < 4 {
        return None;
    }
    let from = Square::from_algebraic(&uci[0..2])?;
    let to = Square::from_algebraic(&uci[2..4])?;
    Some((from.0, to.0))
}

/// Inverse of `board_square_from_pos` — center of a square in screen coords.
pub(crate) fn board_square_center(
    board_rect: egui::Rect,
    sq_size: f32,
    flipped: bool,
    sq_idx: u8,
) -> egui::Pos2 {
    let display_rank = sq_idx / 8;
    let display_file = sq_idx % 8;
    let rank = if flipped { display_rank } else { 7 - display_rank };
    let file = if flipped { 7 - display_file } else { display_file };
    egui::pos2(
        board_rect.min.x + (file as f32 + 0.5) * sq_size,
        board_rect.min.y + (rank as f32 + 0.5) * sq_size,
    )
}

/// The outline of a board arrow from one square center to another: seven
/// corners, clockwise on screen, starting at the tip. `None` when the two
/// points coincide.
///
/// The order matters. An arrow is not convex, but egui fills a closed path
/// as a triangle fan from its first point, and every corner of an arrow can
/// be seen from its tip, so the fan covers the arrow exactly as long as the
/// tip comes first and the winding is clockwise (egui reverses a
/// counter-clockwise path, which would move the fan to a barb corner).
pub(crate) fn annotation_arrow_outline(
    from: egui::Pos2,
    to: egui::Pos2,
    sq_size: f32,
) -> Option<Vec<egui::Pos2>> {
    let dir = to - from;
    let len = dir.length();
    if len < 1.0 {
        return None;
    }
    let unit = dir / len;
    // Clockwise side of the direction of travel, in screen coordinates.
    let perp = egui::vec2(-unit.y, unit.x);

    let half_shaft = sq_size * 0.09;
    let head_len = sq_size * 0.36;
    let head_half_width = sq_size * 0.26;

    // Pull both endpoints in so the arrow sits inside the squares, not flush
    // against the edges (looks better visually).
    let tail = from + unit * (sq_size * 0.18);
    let tip = to - unit * (sq_size * 0.10);
    let head_base = tip - unit * head_len;

    Some(vec![
        tip,
        head_base + perp * head_half_width,
        head_base + perp * half_shaft,
        tail + perp * half_shaft,
        tail - perp * half_shaft,
        head_base - perp * half_shaft,
        head_base - perp * head_half_width,
    ])
}

/// Draw a thick colored arrow with a triangular arrowhead, used for the
/// right-click annotations and the Game Review move arrows.
pub(crate) fn draw_annotation_arrow(
    painter: &egui::Painter,
    from: egui::Pos2,
    to: egui::Pos2,
    color: egui::Color32,
    sq_size: f32,
) {
    let Some(outline) = annotation_arrow_outline(from, to, sq_size) else {
        return;
    };

    // Shaft and head are filled as one shape, so the translucent color has
    // the same strength everywhere. As two overlapping shapes they left a
    // darker patch inside the head.
    painter.add(egui::Shape::convex_polygon(
        outline.clone(),
        color,
        egui::Stroke::NONE,
    ));

    // Thin dark outline around the whole arrow so it reads on every board
    // theme: a green arrow on the green board, a yellow one on maple. The
    // arrow colors carry meaning (green is the best move), so they stay the
    // same on all boards and the outline does the separating.
    painter.add(egui::Shape::closed_line(
        outline,
        egui::Stroke::new(
            (sq_size * 0.014).clamp(1.0, 2.0),
            egui::Color32::from_rgba_unmultiplied(24, 20, 16, 150),
        ),
    ));
}

pub(crate) fn board_square_from_pos(
    board_rect: egui::Rect,
    sq_size: f32,
    flipped: bool,
    pos: egui::Pos2,
) -> Option<u8> {
    if !board_rect.contains(pos) {
        return None;
    }

    let file = ((pos.x - board_rect.min.x) / sq_size).floor() as u8;
    let rank = ((pos.y - board_rect.min.y) / sq_size).floor() as u8;
    if file >= 8 || rank >= 8 {
        return None;
    }

    let display_file = if flipped { 7 - file } else { file };
    let display_rank = if flipped { rank } else { 7 - rank };
    Some(display_rank * 8 + display_file)
}

/// Screen rect of a board square — the inverse of `board_square_from_pos`.
pub(crate) fn board_square_rect(
    board_rect: egui::Rect,
    sq_size: f32,
    flipped: bool,
    sq_idx: u8,
) -> egui::Rect {
    let rank = sq_idx / 8;
    let file = sq_idx % 8;
    let screen_file = if flipped { 7 - file } else { file };
    let screen_rank = if flipped { rank } else { 7 - rank };
    egui::Rect::from_min_size(
        egui::pos2(
            board_rect.min.x + f32::from(screen_file) * sq_size,
            board_rect.min.y + f32::from(screen_rank) * sq_size,
        ),
        egui::vec2(sq_size, sq_size),
    )
}

pub(crate) fn draw_piece_image(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    rect: egui::Rect,
) {
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// Corner radius for one cell of a painted grid whose last row and column
/// index is `last`, so that only the grid's four outer corners are rounded.
pub(crate) fn grid_cell_corner_radius(col: u8, row: u8, last: u8, radius: u8) -> egui::CornerRadius {
    egui::CornerRadius {
        nw: if col == 0 && row == 0 { radius } else { 0 },
        ne: if col == last && row == 0 { radius } else { 0 },
        sw: if col == 0 && row == last { radius } else { 0 },
        se: if col == last && row == last { radius } else { 0 },
    }
}

/// One clickable board-theme swatch for the Settings window: a 4x4 sample of
/// the preset's two square colors with its name underneath. The selected
/// preset gets an accent ring.
pub(crate) fn board_theme_swatch(ui: &mut egui::Ui, theme: BoardTheme, selected: bool) -> egui::Response {
    let side = SETTINGS_SWATCH;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(side, side + SETTINGS_SWATCH_LABEL_H),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter();
    let sample = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
    let (light, dark) = theme.squares();
    let cell = side / 4.0;
    for row in 0..4u8 {
        for col in 0..4u8 {
            // Top-left is a light square, like a8 on a real board.
            let color = if (row + col) % 2 == 0 { light } else { dark };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(
                        sample.min.x + col as f32 * cell,
                        sample.min.y + row as f32 * cell,
                    ),
                    egui::vec2(cell, cell),
                ),
                grid_cell_corner_radius(col, row, 3, 5),
                color,
            );
        }
    }

    finish_swatch(painter, sample, response.hovered(), selected, theme.label());

    response
}

/// One clickable piece-set swatch for the Settings window: four pieces of
/// the set on a 2x2 corner of the current board, so a set is judged on the
/// squares it will actually sit on. Same footprint and ring as the board
/// swatches.
pub(crate) fn piece_set_swatch(
    ui: &mut egui::Ui,
    set: PieceSet,
    textures: &PieceTextures,
    palette: BoardPalette,
    selected: bool,
) -> egui::Response {
    let side = SETTINGS_SWATCH;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(side, side + SETTINGS_SWATCH_LABEL_H),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter();
    let sample = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
    let cell = side / 2.0;
    // Both sides on both square colors: king and queen up top, knights below.
    let pieces = [
        [(Color::White, Piece::King), (Color::Black, Piece::Queen)],
        [(Color::White, Piece::Knight), (Color::Black, Piece::Knight)],
    ];
    for row in 0..2u8 {
        for col in 0..2u8 {
            let square = egui::Rect::from_min_size(
                egui::pos2(
                    sample.min.x + col as f32 * cell,
                    sample.min.y + row as f32 * cell,
                ),
                egui::vec2(cell, cell),
            );
            let color = if (row + col) % 2 == 0 { palette.light } else { palette.dark };
            painter.rect_filled(square, grid_cell_corner_radius(col, row, 1, 5), color);
            let (piece_color, piece) = pieces[row as usize][col as usize];
            if let Some(texture) = textures.small.get(&(piece_color, piece)) {
                draw_piece_image(painter, texture, square.shrink(cell * 0.05));
            }
        }
    }

    finish_swatch(painter, sample, response.hovered(), selected, set.label());

    response
}

/// Accent ring around the selected swatch (a subtle one on hover) and the
/// name under it. Shared by the board-theme and piece-set swatches.
pub(crate) fn finish_swatch(
    painter: &egui::Painter,
    sample: egui::Rect,
    hovered: bool,
    selected: bool,
    label: &str,
) {
    if selected {
        painter.rect_stroke(
            sample.expand(3.0),
            egui::CornerRadius::same(8),
            egui::Stroke::new(2.0_f32, hydra_accent()),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_stroke(
            sample.expand(3.0),
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0_f32, hydra_subtle_text()),
            egui::StrokeKind::Inside,
        );
    }

    let (font, color) = if selected {
        (egui::FontId::new(11.5, hydra_heading_family()), hydra_text())
    } else {
        (egui::FontId::proportional(11.5), hydra_subtle_text())
    };
    painter.text(
        egui::pos2(sample.center().x, sample.max.y + SETTINGS_SWATCH_LABEL_H * 0.6),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
}

/// One promotion candidate: the piece image on a board square with its name
/// underneath and a ring on hover. Same footprint as the Settings swatches.
pub(crate) fn promotion_tile(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    square_color: egui::Color32,
    label: &str,
) -> egui::Response {
    let side = SETTINGS_SWATCH;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(side, side + SETTINGS_SWATCH_LABEL_H),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter();
    let sample = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
    painter.rect_filled(sample, egui::CornerRadius::same(5), square_color);
    if let Some(texture) = texture {
        draw_piece_image(painter, texture, sample.shrink(side * 0.06));
    }
    finish_swatch(painter, sample, response.hovered(), false, label);

    response
}

/// Compact non-interactive board renderer for History thumbnails. Always
/// shown white-on-bottom. Pieces use Unicode glyphs to avoid loading the
/// big piece textures at thumbnail scale.
pub(crate) fn draw_board_thumbnail(ui: &mut egui::Ui, board: &Board, size: f32) {
    let (response, painter) = ui.allocate_painter(
        egui::vec2(size, size),
        egui::Sense::hover(),
    );
    let rect = response.rect;
    let sq = size / 8.0;
    let palette = active_board_theme().palette();
    let light = palette.light;
    let dark = palette.dark;

    for visual_rank in 0..8u8 {
        // visual_rank 0 = top of board = chess rank 8 = file index 7
        let chess_rank = 7 - visual_rank;
        for file in 0..8u8 {
            let sq_idx = chess_rank * 8 + file;
            let is_light = (file + chess_rank) % 2 == 1;
            let color = if is_light { light } else { dark };
            let x = rect.min.x + file as f32 * sq;
            let y = rect.min.y + visual_rank as f32 * sq;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(sq, sq)),
                0.0,
                color,
            );
            if let Some((color_p, piece)) = board.piece_on(Square(sq_idx)) {
                let glyph = piece_unicode(color_p, piece);
                let text_color = if color_p == Color::White {
                    egui::Color32::from_rgb(250, 250, 250)
                } else {
                    egui::Color32::from_rgb(20, 20, 20)
                };
                painter.text(
                    egui::pos2(x + sq / 2.0, y + sq / 2.0),
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    egui::FontId::proportional(sq * 0.85),
                    text_color,
                );
            }
        }
    }
}

pub(crate) fn piece_unicode(color: Color, piece: Piece) -> &'static str {
    match (color, piece) {
        (Color::White, Piece::King) => "\u{2654}",
        (Color::White, Piece::Queen) => "\u{2655}",
        (Color::White, Piece::Rook) => "\u{2656}",
        (Color::White, Piece::Bishop) => "\u{2657}",
        (Color::White, Piece::Knight) => "\u{2658}",
        (Color::White, Piece::Pawn) => "\u{2659}",
        (Color::Black, Piece::King) => "\u{265A}",
        (Color::Black, Piece::Queen) => "\u{265B}",
        (Color::Black, Piece::Rook) => "\u{265C}",
        (Color::Black, Piece::Bishop) => "\u{265D}",
        (Color::Black, Piece::Knight) => "\u{265E}",
        (Color::Black, Piece::Pawn) => "\u{265F}",
    }
}

