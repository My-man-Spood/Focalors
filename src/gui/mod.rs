use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::attacks;
use crate::board::Board;
use crate::movegen::{generate_legal_moves, make_move};
use crate::moves::{Move, MoveFlag};
use crate::search::Searcher;
use crate::types::*;
use crate::uci;

mod history;
mod home;
mod play;
mod puzzle;
mod review;
mod settings;
mod stats;
mod board_render;
mod game_state;
mod omarchy;
mod style;

use board_render::*;
use game_state::*;
use omarchy::OmarchyPalette;
use style::*;
use home::{LocalDifficulty, SideChoice, TimePreset};
use review::AnalysisQuality;

// ════════════════════════════════════════════════════════════════════════════
// Shared state between GUI and background threads
// ════════════════════════════════════════════════════════════════════════════

/// How the Custom opponent thinks: a fixed time per move or a fixed depth.
/// Only the Custom profile reads these; the named profiles carry their own
/// strength settings. Persisted in settings.toml.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EngineSettings {
    pub max_depth: u32,
    pub think_time_ms: u64,
    pub use_time_limit: bool, // true = time-based, false = depth-based
}

impl EngineSettings {
    /// Slider ranges in the Custom opponent window. Stored values are
    /// clamped into them, so an edited or corrupted row cannot produce a
    /// zero-depth or hour-long search.
    pub const THINK_TIME_MS_RANGE: (u64, u64) = (500, 30_000);
    pub const MAX_DEPTH_RANGE: (u32, u32) = (1, 30);

    /// Rebuild from the persisted columns, clamping into the slider ranges.
    pub fn from_stored(use_time_limit: bool, think_time_ms: i64, max_depth: i64) -> Self {
        let (t_min, t_max) = Self::THINK_TIME_MS_RANGE;
        let (d_min, d_max) = Self::MAX_DEPTH_RANGE;
        Self {
            max_depth: max_depth.clamp(d_min as i64, d_max as i64) as u32,
            think_time_ms: think_time_ms.clamp(t_min as i64, t_max as i64) as u64,
            use_time_limit,
        }
    }
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            max_depth: 12,
            think_time_ms: 5000,
            use_time_limit: true,
        }
    }
}

#[derive(Clone)]
struct SearchInfo {
    depth: u32,
    score: i32,
    nodes: u64,
    best_move: String,
    searching: bool,
}

impl Default for SearchInfo {
    fn default() -> Self {
        Self {
            depth: 0,
            score: 0,
            nodes: 0,
            best_move: String::new(),
            searching: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HomePage {
    Overview,
    Analyze,
    Progress,
    Statistics,
    History,
    Puzzles,
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(0);

/// Set only while `UiTheme::System` resolves to Omarchy's own palette
/// (see `resolve_system_theme`); `None` otherwise, including whenever a
/// concrete Dark or Light is picked directly. The `hydra_*` chrome colors
/// in `style.rs` check this before falling back to the built-in palette —
/// a `Mutex` rather than another atomic because a color doesn't fit in a
/// byte, but it's read far more than it's written, so contention is a
/// non-issue.
static ACTIVE_OMARCHY_PALETTE: Mutex<Option<OmarchyPalette>> = Mutex::new(None);

// Highlight colors shared by every board theme. They are the exact colors
// the walnut board showed in dark mode back when the highlights were
// translucent tints painted over the panel behind the board. Spelled out as
// opaque colors they no longer depend on the UI theme (the tints washed out
// to pale yellow and white in light mode), and one flat gold reads cleanly on
// every preset, where a tint blended into blue or teal squares turns muddy.
const BOARD_SELECTED: egui::Color32 = egui::Color32::from_rgb(227, 191, 107);
const BOARD_DRAG_HOVER: egui::Color32 = egui::Color32::from_rgb(218, 169, 98);
const BOARD_LAST_MOVE: egui::Color32 = egui::Color32::from_rgb(230, 201, 121);
const BOARD_LEGAL_DOT: egui::Color32 = egui::Color32::from_rgb(255, 255, 253);

static ACTIVE_BOARD_THEME: AtomicU8 = AtomicU8::new(0);

/// Position shown in the Settings window's board preview: a Ruy Lopez after
/// 3...Nf6, so both colors have pieces off their home squares.
const SETTINGS_PREVIEW_FEN: &str =
    "r1bqkb1r/pppp1ppp/2n2n2/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
/// Squares of the move that led to the preview position (g8 and f6), drawn
/// with the last-move highlight.
const SETTINGS_PREVIEW_LAST_MOVE: [u8; 2] = [62, 45];


#[derive(Clone)]
struct TimeControl {
    label: &'static str,
    initial_time_ms: u64,
    increment_ms: u64,
    white_remaining_ms: u64,
    black_remaining_ms: u64,
    active_since: Option<Instant>,
}

impl TimeControl {
    fn from_preset(preset: TimePreset) -> Self {
        let initial_ms = preset.initial_ms();
        Self {
            label: preset.label(),
            initial_time_ms: initial_ms,
            increment_ms: preset.increment_ms(),
            white_remaining_ms: initial_ms,
            black_remaining_ms: initial_ms,
            active_since: None,
        }
    }

    fn clear_active(&mut self) {
        self.active_since = None;
    }

    fn start_turn_now(&mut self) {
        self.active_since = Some(Instant::now());
    }

    fn remaining_ms(&self, side: Color) -> u64 {
        match side {
            Color::White => self.white_remaining_ms,
            Color::Black => self.black_remaining_ms,
        }
    }

    fn displayed_remaining_ms(&self, side: Color, active_side: Option<Color>) -> u64 {
        let remaining = self.remaining_ms(side);
        if active_side == Some(side) {
            if let Some(active_since) = self.active_since {
                return remaining.saturating_sub(elapsed_ms(active_since));
            }
        }
        remaining
    }

    fn consume_running_time(&mut self, side: Color) -> u64 {
        let elapsed = self.active_since.take().map(elapsed_ms).unwrap_or(0);
        let remaining = match side {
            Color::White => &mut self.white_remaining_ms,
            Color::Black => &mut self.black_remaining_ms,
        };
        *remaining = remaining.saturating_sub(elapsed);
        *remaining
    }

    fn add_increment(&mut self, side: Color) {
        let remaining = match side {
            Color::White => &mut self.white_remaining_ms,
            Color::Black => &mut self.black_remaining_ms,
        };
        *remaining = remaining.saturating_add(self.increment_ms);
    }

    fn flag_side(&mut self, side: Color) {
        match side {
            Color::White => self.white_remaining_ms = 0,
            Color::Black => self.black_remaining_ms = 0,
        }
        self.clear_active();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameOutcome {
    Checkmate(Color),
    Stalemate,
    FiftyMoveRule,
    ThreefoldRepetition,
    InsufficientMaterial,
    Timeout(Color),
    Resignation(Color),
}

#[derive(Clone)]
struct LocalGameState {
    active: bool,
    human_color: Color,
    difficulty: LocalDifficulty,
    numeric_level: u32,
    outcome: Option<GameOutcome>,
    time_control: TimeControl,
}

impl LocalGameState {
    fn idle(human_color: Color, time_preset: TimePreset, difficulty: LocalDifficulty, level: u32) -> Self {
        Self {
            active: false,
            human_color,
            difficulty,
            numeric_level: level,
            outcome: None,
            time_control: TimeControl::from_preset(time_preset),
        }
    }

    fn new(human_color: Color, time_preset: TimePreset, difficulty: LocalDifficulty, level: u32) -> Self {
        let mut time_control = TimeControl::from_preset(time_preset);
        time_control.start_turn_now();

        Self {
            active: true,
            human_color,
            difficulty,
            numeric_level: level,
            outcome: None,
            time_control,
        }
    }
}

impl Default for LocalGameState {
    fn default() -> Self {
        Self::idle(Color::White, TimePreset::Rapid10_0, LocalDifficulty::default(), crate::strength::TOURNAMENT_LEVEL)
    }
}

#[derive(Clone)]
struct LocalSnapshot {
    board: Board,
    white_remaining_ms: u64,
    black_remaining_ms: u64,
    move_uci: Option<String>,
}

#[derive(Clone)]
struct PendingPromotion {
    moves: Vec<Move>,
}

#[derive(Clone, Copy)]
struct DragState {
    from_sq: u8,
    piece_color: Color,
    piece: Piece,
    pointer_pos: egui::Pos2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AnnotationColor {
    Green,
    Red,
    Yellow,
    Blue,
}

impl AnnotationColor {
    fn from_modifiers(m: egui::Modifiers) -> Self {
        // Shift > command/ctrl > alt > plain. Matches lichess/chess.com convention.
        if m.shift { Self::Red }
        else if m.command { Self::Yellow }
        else if m.alt { Self::Blue }
        else { Self::Green }
    }

    fn fill(self) -> egui::Color32 {
        // ~63% alpha — visible over both light and dark squares without
        // obliterating the piece beneath.
        match self {
            Self::Green  => egui::Color32::from_rgba_premultiplied(70, 150, 70, 160),
            Self::Red    => egui::Color32::from_rgba_premultiplied(200, 70, 70, 160),
            Self::Yellow => egui::Color32::from_rgba_premultiplied(210, 165, 60, 160),
            Self::Blue   => egui::Color32::from_rgba_premultiplied(70, 130, 200, 160),
        }
    }

    fn arrow(self) -> egui::Color32 {
        // Slightly more opaque for arrows so the shape reads cleanly over pieces.
        match self {
            Self::Green  => egui::Color32::from_rgba_premultiplied(70, 150, 70, 210),
            Self::Red    => egui::Color32::from_rgba_premultiplied(200, 70, 70, 210),
            Self::Yellow => egui::Color32::from_rgba_premultiplied(210, 165, 60, 210),
            Self::Blue   => egui::Color32::from_rgba_premultiplied(70, 130, 200, 210),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Annotation {
    Highlight { sq: u8, color: AnnotationColor },
    Arrow { from: u8, to: u8, color: AnnotationColor },
}

#[derive(Clone)]
struct LocalSearchRequest {
    soft_time_ms: u64,
    hard_time_ms: u64,
    depth_cap: Option<u32>,
    generation: u64,
    strength_config: crate::strength::StrengthConfig,
}

/// State for replaying a saved game.
#[derive(Clone)]
struct ReplayState {
    game: crate::db::SavedGame,
    /// Parsed UCI move list, lazily populated on first open.
    uci_moves: Vec<String>,
    /// Cached positions: `boards[i]` is the position before ply `i`.
    /// `boards[0]` is the start position; `boards[uci_moves.len()]` is the final position.
    boards: Vec<Board>,
    /// Move objects parallel to `uci_moves`, used for last-move highlighting.
    moves: Vec<Move>,
    /// Currently displayed ply index, in `0..=uci_moves.len()`.
    cursor: usize,
}

/// How long a piece glides between squares. Short enough to never lag
/// behind fast play, long enough that moves read as motion, not teleports.
const MOVE_ANIM_DURATION: Duration = Duration::from_millis(160);

/// A piece glide queued at move-apply time, before `make_move`, by whichever
/// thread applied the move (GUI thread for human moves, worker for engine
/// replies). Captured pre-move so the renderer never has to reconstruct
/// which piece moved. `rook` is the companion (from, to) glide for castling.
#[derive(Clone, Copy)]
struct PendingAnimation {
    from_sq: u8,
    to_sq: u8,
    piece: (Color, Piece),
    rook: Option<(u8, u8)>,
    /// Whether this move captured a piece (including en passant), decided
    /// at apply-time from the before-move board — by drain time the board
    /// already reflects the after state, so this can't be re-derived later.
    /// Drives the move/capture sound choice; see `move_sfx`.
    captured: bool,
    /// Whether this payload should also start a visual glide. False for
    /// drag-drops, which are already drawn at their destination square —
    /// the sound still fires either way, using `captured` above.
    animate: bool,
}

/// A glide in flight on the live board: `pending.piece` slides from-square
/// to to-square over `MOVE_ANIM_DURATION`, hiding the real piece on the
/// destination square until it lands. Purely visual — the board state has
/// already moved on, so dropping one mid-flight just snaps the piece home.
struct MoveAnimation {
    pending: PendingAnimation,
    start: Instant,
}

pub(crate) struct BoardView<'a> {
    board: &'a Board,
    last_move: Option<Move>,
    interactive: bool,
    /// Game Review overlay: best-move and played-move arrows for the move that
    /// produced this position. `None` for live play / anything non-review.
    review: Option<ReviewOverlay>,
    /// Showing a hypothetical position from hovering a continuation move,
    /// not the real game state — draw_board marks the board shell so it's
    /// never mistaken for "this actually happened."
    preview: bool,
}

/// Overlay drawn on the read-only Game Review board for the move that produced
/// the displayed position: the engine's best move (green arrow), the played
/// move (classification-colored arrow, suppressed when it equals best), and a
/// classification badge on the played move's destination square. Square pairs
/// are (from, to) indices.
#[derive(Clone, Copy)]
struct ReviewOverlay {
    best: Option<(u8, u8)>,
    played: Option<(u8, u8)>,
    played_color: egui::Color32,
    played_class: crate::analysis::MoveClass,
}

/// Puzzle trainer state — active when the user is solving puzzles.
struct PuzzleTrainerState {
    puzzle: crate::db::SavedPuzzle,
    board: Board,
    solution_move: Move,
    user_attempted: bool,
    user_solved: bool,
    show_hint: bool,
    show_answer: bool,
    wrong_attempts: u32,
}

/// Analysis state — shared between the GUI and the background analysis thread.
#[derive(Clone)]
enum AnalysisState {
    Idle,
    Running {
        progress: usize, // moves analyzed so far
        total: usize,
    },
    Complete {
        analysis: crate::analysis::GameAnalysis,
        puzzles: Vec<crate::puzzles::PuzzleCandidate>,
        uci_moves: Vec<String>,
        /// The game this analysis belongs to, captured when the worker
        /// was SPAWNED — never read from current UI state at persist
        /// time, or a stale worker finishing late would save game A's
        /// moves under whatever game the user has navigated to since.
        game_id: Option<i64>,
    },
}

struct SharedState {
    board: Board,
    move_history: Vec<String>,
    local_history: Vec<LocalSnapshot>,
    local_history_cursor: usize,
    local_search_generation: u64,
    engine_settings: EngineSettings,
    search_info: SearchInfo,
    local_game: LocalGameState,
    game_saved: bool,
    /// Persistent searcher for TT reuse across moves
    persistent_searcher: Arc<Mutex<Searcher>>,
    /// Glide-and-sound payloads queued by move application, drained by
    /// `draw_board` (playback needs the audio device on `FocalorsApp`, not
    /// reachable from the background search thread that applies the
    /// engine's move).
    pending_animations: Vec<PendingAnimation>,
    status_message: String,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            board: Board::startpos(),
            move_history: Vec::new(),
            local_history: Vec::new(),
            local_history_cursor: 0,
            local_search_generation: 0,
            engine_settings: EngineSettings::default(),
            search_info: SearchInfo::default(),
            local_game: LocalGameState::default(),
            game_saved: false,
            persistent_searcher: {
                let mut s = Searcher::new(64);
                s.use_nnue = crate::nnue::network::get_network().is_some();
                Arc::new(Mutex::new(s))
            },
            pending_animations: Vec::new(),
            status_message: "Idle. Choose local play to begin.".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Main GUI app
// ════════════════════════════════════════════════════════════════════════════

pub struct FocalorsApp {
    state: Arc<Mutex<SharedState>>,
    db: Option<crate::db::Database>,
    profile: Option<crate::db::UserProfile>,
    recent_games: Vec<crate::db::SavedGame>,
    result_counts: (i32, i32, i32), // wins, losses, draws
    show_welcome: bool,
    welcome_name: String,
    welcome_rating_choice: i32,
    home_page: HomePage,
    ui_theme: UiTheme,
    board_theme: BoardTheme,
    piece_set: PieceSet,
    analysis_quality: AnalysisQuality,
    show_settings: bool,
    /// Fixed position drawn in the Settings window's board preview.
    settings_preview_board: Board,
    replay_game: Option<ReplayState>,
    analysis_state: Arc<Mutex<AnalysisState>>,
    analysis_review_cursor: usize, // which move is selected in review
    /// Set for one frame when the previous frame's continuation row had a
    /// ply hovered — swaps the review board for that hypothetical position
    /// instead of the real one. One-frame-lagged like every other
    /// cross-section dependency in `draw_analyze_page` (e.g. move-list
    /// clicks vs. the board), since the continuation row renders after the
    /// board does.
    review_preview_ply: Option<usize>,
    /// If set, completed analysis should be persisted against this game id
    /// instead of the most-recent-game fallback.
    analysis_target_game_id: Option<i64>,
    /// Monotonic stamp for analysis workers; see start_analysis.
    analysis_generation: Arc<AtomicU64>,
    /// Cache of (game_id → final-position board) for History thumbnails.
    /// Computed lazily on first display, never invalidated since saved game
    /// PGNs don't change after persistence.
    history_thumbnails: std::collections::HashMap<i64, Option<Board>>,
    selected_square: Option<u8>,
    drag_state: Option<DragState>,
    /// Piece glides currently in flight on the live board.
    move_animations: Vec<MoveAnimation>,
    /// User-drawn annotations (right-click highlights + arrows for tactical
    /// thinking). Cleared on left-click, on move played, and on board change.
    annotations: Vec<Annotation>,
    /// Square where a right-click drag began. Some while the right button is
    /// held down; resolved into an annotation on release.
    right_drag_origin: Option<u8>,
    /// Last-seen board hash. When this changes between frames, annotations
    /// are wiped (a new position invalidates whatever the user drew before).
    last_annotation_board_hash: Option<u64>,
    flipped: bool,
    local_side_choice: SideChoice,
    local_time_preset: TimePreset,
    local_difficulty: LocalDifficulty,
    adaptive_level: u32,
    auto_adjust_message: Option<(String, std::time::Instant)>,
    /// Rating before/after the most recently saved live game, shown in
    /// the session card under the outcome banner.
    last_rating_change: Option<(i32, i32)>,
    show_eval_panel: bool,
    session_start_rating: i32,
    session_start_games: i32,
    session_best_accuracy: Option<f64>,
    puzzle_trainer: Option<PuzzleTrainerState>,
    puzzle_message: Option<(String, egui::Color32, std::time::Instant)>,
    show_advanced_engine_settings: bool,
    /// Custom opponent values changed in its window but not yet written to
    /// the profile row. Flushed once the slider is released or the window
    /// closes, so a drag does not write on every frame.
    custom_engine_dirty: bool,
    /// Resign and End Game each ask for a second click: set on the first
    /// click, and the button reads "Confirm ..." until this instant passes.
    resign_confirm_until: Option<Instant>,
    end_game_confirm_until: Option<Instant>,
    pending_promotion: Option<PendingPromotion>,
    /// Textures of every piece set, loaded once at startup so Settings can
    /// show all of them and switching never stalls a frame.
    piece_sets: HashMap<PieceSet, PieceTextures>,
    pgn_import_text: String,
    pgn_import_parsed: Option<crate::pgn::ParsedPgn>,
    pgn_import_error: Option<String>,
    pgn_import_user_color: Color,
    /// Cache for the live PGN parse, keyed on the exact import text.
    /// parse_pgn replays the whole game (legal movegen per SAN token),
    /// so re-running it every frame while text sits in the box wastes
    /// milliseconds per frame on long games.
    pgn_parse_cache: Option<(String, Result<crate::pgn::ParsedPgn, String>)>,
    /// Move/capture/check/game-end SFX. `None` if no audio device could be
    /// opened, in which case sound is just silently absent.
    audio: Option<crate::sound::AudioOutput>,
    sfx_volume: f32,
    /// Set while the Settings volume slider is being dragged; flushed to
    /// the DB on release, like `custom_engine_dirty`, so dragging doesn't
    /// write on every frame.
    sfx_volume_dirty: bool,
    /// Whether the game-end SFX has already fired for the local game's
    /// current outcome. Checked each frame in `draw_board` so the sound
    /// plays no matter how the game ended — checkmate, resignation,
    /// timeout, draw — not just when the ending move was the trigger.
    game_end_sound_played: bool,
    /// Rate-limits re-checking Omarchy/the OS theme signal while
    /// `UiTheme::System` is active, so a live Omarchy theme switch is
    /// picked up without re-reading two files every single frame.
    last_system_theme_poll: Option<Instant>,
}

macro_rules! piece_set_images {
    ($dir:literal) => {
        [
            (Color::White, Piece::King, "wK", include_bytes!(concat!("../../assets/pieces/", $dir, "/wK.png"))),
            (Color::White, Piece::Queen, "wQ", include_bytes!(concat!("../../assets/pieces/", $dir, "/wQ.png"))),
            (Color::White, Piece::Rook, "wR", include_bytes!(concat!("../../assets/pieces/", $dir, "/wR.png"))),
            (Color::White, Piece::Bishop, "wB", include_bytes!(concat!("../../assets/pieces/", $dir, "/wB.png"))),
            (Color::White, Piece::Knight, "wN", include_bytes!(concat!("../../assets/pieces/", $dir, "/wN.png"))),
            (Color::White, Piece::Pawn, "wP", include_bytes!(concat!("../../assets/pieces/", $dir, "/wP.png"))),
            (Color::Black, Piece::King, "bK", include_bytes!(concat!("../../assets/pieces/", $dir, "/bK.png"))),
            (Color::Black, Piece::Queen, "bQ", include_bytes!(concat!("../../assets/pieces/", $dir, "/bQ.png"))),
            (Color::Black, Piece::Rook, "bR", include_bytes!(concat!("../../assets/pieces/", $dir, "/bR.png"))),
            (Color::Black, Piece::Bishop, "bB", include_bytes!(concat!("../../assets/pieces/", $dir, "/bB.png"))),
            (Color::Black, Piece::Knight, "bN", include_bytes!(concat!("../../assets/pieces/", $dir, "/bN.png"))),
            (Color::Black, Piece::Pawn, "bP", include_bytes!(concat!("../../assets/pieces/", $dir, "/bP.png"))),
        ]
    };
}

const CBURNETT_IMAGES: [(Color, Piece, &str, &[u8]); 12] = piece_set_images!("cburnett");
const RHOSGFX_IMAGES: [(Color, Piece, &str, &[u8]); 12] = piece_set_images!("rhosgfx");

/// Edge length of the downscaled piece textures used on small boards (the
/// Settings preview and piece-set swatches). The full-size textures alias
/// badly once the GPU has to shrink them more than about 2x with plain
/// linear filtering.
const PIECE_SMALL_PX: u32 = 48;

fn load_piece_texture(
    ctx: &egui::Context,
    name: &str,
    data: &[u8],
) -> egui::TextureHandle {
    let img = image::load_from_memory(data)
        .expect("Failed to load piece image")
        .into_rgba8();
    let size = [img.width() as _, img.height() as _];
    let pixels = img.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

/// Same image as `load_piece_texture`, pre-shrunk to `PIECE_SMALL_PX` with a
/// proper resampling filter.
fn load_small_piece_texture(
    ctx: &egui::Context,
    name: &str,
    data: &[u8],
) -> egui::TextureHandle {
    let mut img = image::load_from_memory(data)
        .expect("Failed to load piece image")
        .into_rgba8();
    // Resize in premultiplied alpha so the color stored in fully transparent
    // pixels can't bleed into the piece outline.
    for px in img.pixels_mut() {
        let alpha = px[3] as u32;
        for c in 0..3 {
            px[c] = ((px[c] as u32 * alpha + 127) / 255) as u8;
        }
    }
    let mut small = image::imageops::resize(
        &img,
        PIECE_SMALL_PX,
        PIECE_SMALL_PX,
        image::imageops::FilterType::Lanczos3,
    );
    // Lanczos can overshoot slightly; keep the result valid premultiplied.
    for px in small.pixels_mut() {
        let alpha = px[3];
        for c in 0..3 {
            px[c] = px[c].min(alpha);
        }
    }
    let size = [PIECE_SMALL_PX as usize, PIECE_SMALL_PX as usize];
    let color_image = egui::ColorImage::from_rgba_premultiplied(size, small.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

/// Full-size and downscaled textures of one piece set, both keyed by
/// (side, piece).
struct PieceTextures {
    full: HashMap<(Color, Piece), egui::TextureHandle>,
    small: HashMap<(Color, Piece), egui::TextureHandle>,
}

fn load_piece_set(ctx: &egui::Context, set: PieceSet) -> PieceTextures {
    let dir = set.as_str();
    let mut full = HashMap::new();
    let mut small = HashMap::new();
    for &(color, piece, name, data) in set.images() {
        full.insert(
            (color, piece),
            load_piece_texture(ctx, &format!("{dir}/{name}"), data),
        );
        small.insert(
            (color, piece),
            load_small_piece_texture(ctx, &format!("{dir}/{name}_small"), data),
        );
    }
    PieceTextures { full, small }
}

impl FocalorsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Theme is configured below once the DB has been opened and the
        // persisted preference loaded — so the very first frame already
        // reflects the user's saved choice instead of always being Light.

        // Diagnostic for the "settings keep resetting" investigation: if
        // eframe ever recreates the app within one process (a renderer
        // retry, a context loss), each call re-runs pending migrations —
        // this line proves whether that's happening.
        crate::settings::log_event("FocalorsApp::new() called");

        let state = Arc::new(Mutex::new(SharedState::default()));

        // Load every piece set's textures from the embedded PNGs
        let ctx = &cc.egui_ctx;
        install_fonts(ctx);
        let piece_sets: HashMap<PieceSet, PieceTextures> = PieceSet::ALL
            .into_iter()
            .map(|set| (set, load_piece_set(ctx, set)))
            .collect();

        // Initialize database
        let db = match crate::db::Database::open() {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("Warning: Could not open database: {e}");
                None
            }
        };

        let (profile, show_welcome) = if let Some(ref db) = db {
            match db.get_or_create_profile() {
                Ok(p) => {
                    let first_time = p.name == "Player" && p.games_played == 0;
                    (Some(p), first_time)
                }
                Err(_) => (None, true),
            }
        } else {
            (None, false)
        };

        let recent_games = db
            .as_ref()
            .and_then(|db| db.get_recent_games(20).ok())
            .unwrap_or_default();

        let result_counts = db
            .as_ref()
            .and_then(|db| db.get_result_counts().ok())
            .unwrap_or((0, 0, 0));

        let adaptive_level = db
            .as_ref()
            .and_then(|db| db.get_adaptive_level().ok())
            .unwrap_or(10);

        // Preferences live in settings.toml, not the database (see
        // docs/REFACTORING.md item 4) — one load instead of six DB round
        // trips. Falls back to defaults on a missing/corrupt file.
        let loaded_settings = crate::settings::load();

        // Applied to egui's visuals before the first frame.
        let ui_theme = UiTheme::from_str(&loaded_settings.ui_theme);
        configure_theme(&cc.egui_ctx, ui_theme);

        // Applied through a process-wide atomic, like the UI theme.
        let board_theme = BoardTheme::from_str(&loaded_settings.board_theme);
        ACTIVE_BOARD_THEME.store(board_theme.index(), Ordering::Relaxed);

        let piece_set = PieceSet::from_str(&loaded_settings.piece_set);
        let analysis_quality = AnalysisQuality::from_str(&loaded_settings.analysis_quality);
        let sfx_volume = loaded_settings.sfx_volume as f32;

        // Loaded before the first search can start, so a saved Custom
        // setup plays the same after a restart.
        state.lock().unwrap().engine_settings = EngineSettings::from_stored(
            loaded_settings.custom_use_time_limit,
            loaded_settings.custom_think_time_ms,
            loaded_settings.custom_max_depth,
        );

        let session_start_rating = profile.as_ref().map_or(1200, |p| p.rating);
        let session_start_games = profile.as_ref().map_or(0, |p| p.games_played);

        Self {
            state,
            db,
            profile,
            recent_games,
            result_counts,
            show_welcome,
            welcome_name: String::new(),
            welcome_rating_choice: 1200,
            home_page: HomePage::Overview,
            ui_theme,
            board_theme,
            piece_set,
            analysis_quality,
            show_settings: false,
            settings_preview_board: Board::from_fen(SETTINGS_PREVIEW_FEN)
                .unwrap_or_else(|_| Board::startpos()),
            replay_game: None,
            analysis_state: Arc::new(Mutex::new(AnalysisState::Idle)),
            analysis_review_cursor: 0,
            review_preview_ply: None,
            analysis_target_game_id: None,
            analysis_generation: Arc::new(AtomicU64::new(0)),
            history_thumbnails: std::collections::HashMap::new(),
            selected_square: None,
            drag_state: None,
            move_animations: Vec::new(),
            annotations: Vec::new(),
            right_drag_origin: None,
            last_annotation_board_hash: None,
            flipped: false,
            local_side_choice: SideChoice::from_str(&loaded_settings.local_side_choice),
            local_time_preset: TimePreset::from_str(&loaded_settings.local_time_preset),
            local_difficulty: LocalDifficulty::from_str(&loaded_settings.local_difficulty),
            adaptive_level,
            auto_adjust_message: None,
            last_rating_change: None,
            show_eval_panel: false,
            session_start_rating,
            session_start_games,
            session_best_accuracy: None,
            puzzle_trainer: None,
            puzzle_message: None,
            show_advanced_engine_settings: false,
            custom_engine_dirty: false,
            resign_confirm_until: None,
            end_game_confirm_until: None,
            pending_promotion: None,
            piece_sets,
            pgn_import_text: String::new(),
            pgn_import_parsed: None,
            pgn_import_error: None,
            pgn_import_user_color: Color::White,
            pgn_parse_cache: None,
            audio: crate::sound::AudioOutput::open(),
            sfx_volume,
            sfx_volume_dirty: false,
            game_end_sound_played: false,
            last_system_theme_poll: None,
        }
    }

    fn play_sound(&self, sfx: crate::sound::Sfx) {
        if let Some(audio) = &self.audio {
            audio.play(sfx, self.sfx_volume);
        }
    }

    /// Writes every preference currently held on `self` to settings.toml.
    /// TOML has no partial-update story the way `UPDATE ... SET x = ?`
    /// did, so every setter just rewrites the whole file — that's the
    /// idiomatic shape for this format, not a workaround.
    fn save_settings(&self, reason: &str) {
        let engine_settings = self.state.lock().unwrap().engine_settings.clone();
        let settings = crate::settings::Settings {
            ui_theme: self.ui_theme.as_str().to_string(),
            board_theme: self.board_theme.as_str().to_string(),
            piece_set: self.piece_set.as_str().to_string(),
            analysis_quality: self.analysis_quality.as_str().to_string(),
            sfx_volume: self.sfx_volume as f64,
            custom_use_time_limit: engine_settings.use_time_limit,
            custom_think_time_ms: engine_settings.think_time_ms as i64,
            custom_max_depth: engine_settings.max_depth as i64,
            local_side_choice: self.local_side_choice.as_str().to_string(),
            local_time_preset: self.local_time_preset.as_str().to_string(),
            local_difficulty: self.local_difficulty.as_str().to_string(),
        };
        let _ = crate::settings::save(&settings, reason);
    }

    fn set_sfx_volume(&mut self, volume: f32) {
        self.sfx_volume = volume.clamp(0.0, 1.0);
        self.sfx_volume_dirty = true;
    }

    /// While "Match System" is selected, re-checks Omarchy/the OS signal
    /// every couple of seconds so a theme switched live in Omarchy (or an
    /// OS-level light/dark flip) shows up without restarting Focalors or
    /// reopening Settings. Rate-limited rather than watched, since it's
    /// just two small file reads — cheap enough not to need a real
    /// filesystem watcher.
    fn poll_system_theme(&mut self, ctx: &egui::Context) {
        if self.ui_theme != UiTheme::System {
            return;
        }
        let now = Instant::now();
        if self
            .last_system_theme_poll
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(2))
        {
            return;
        }
        self.last_system_theme_poll = Some(now);
        configure_theme(ctx, self.ui_theme);
    }

    fn set_ui_theme(&mut self, ctx: &egui::Context, theme: UiTheme) {
        self.ui_theme = theme;
        configure_theme(ctx, theme);
        // Persist so the choice survives restart. Best-effort: a transient
        // I/O hiccup writing settings.toml never breaks the toggle.
        self.save_settings("set_ui_theme");
    }

    fn set_board_theme(&mut self, theme: BoardTheme) {
        self.board_theme = theme;
        ACTIVE_BOARD_THEME.store(theme.index(), Ordering::Relaxed);
        self.save_settings("set_board_theme");
    }

    fn set_piece_set(&mut self, set: PieceSet) {
        self.piece_set = set;
        self.save_settings("set_piece_set");
    }

    fn set_analysis_quality(&mut self, quality: AnalysisQuality) {
        self.analysis_quality = quality;
        self.save_settings("set_analysis_quality");
    }

    /// Full-size image of a piece in the active set.
    fn piece_texture(&self, color: Color, piece: Piece) -> Option<&egui::TextureHandle> {
        self.piece_sets.get(&self.piece_set)?.full.get(&(color, piece))
    }

    /// Downscaled image of a piece in the active set, for small boards.
    fn small_piece_texture(&self, color: Color, piece: Piece) -> Option<&egui::TextureHandle> {
        self.piece_sets.get(&self.piece_set)?.small.get(&(color, piece))
    }

    fn local_input_enabled(&self) -> bool {
        if self.pending_promotion.is_some() {
            return false;
        }

        // Puzzle mode: allow input if puzzle is active and not solved
        if let Some(ref t) = self.puzzle_trainer {
            return !t.user_solved && !t.show_answer;
        }

        let state = self.state.lock().unwrap();
        state.local_game.active
            && state.local_game.outcome.is_none()
            && !state.search_info.searching
            && state.board.side_to_move == state.local_game.human_color
            && state
                .local_game
                .time_control
                .displayed_remaining_ms(state.board.side_to_move, Some(state.board.side_to_move))
                > 0
    }

    /// Toggle/replace an annotation. If the exact same annotation (same
    /// color + shape) already exists, remove it. If a same-shape annotation
    /// exists with a different color, replace it. Otherwise add.
    fn toggle_annotation(&mut self, new_annot: Annotation) {
        let existing_idx = self.annotations.iter().position(|&a| {
            match (a, new_annot) {
                (Annotation::Highlight { sq: s1, .. }, Annotation::Highlight { sq: s2, .. }) => {
                    s1 == s2
                }
                (
                    Annotation::Arrow { from: f1, to: t1, .. },
                    Annotation::Arrow { from: f2, to: t2, .. },
                ) => f1 == f2 && t1 == t2,
                _ => false,
            }
        });

        match existing_idx {
            Some(idx) if self.annotations[idx] == new_annot => {
                self.annotations.remove(idx);
            }
            Some(idx) => {
                self.annotations[idx] = new_annot;
            }
            None => {
                self.annotations.push(new_annot);
            }
        }
    }

    fn has_running_local_clock(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.local_game.active
            && state.local_game.outcome.is_none()
            && state.local_game.time_control.active_since.is_some()
    }

    fn update_local_clock_state(&mut self) {
        let mut state = self.state.lock().unwrap();
        if handle_active_side_timeout(&mut state) {
            self.pending_promotion = None;
            self.selected_square = None;
            self.drag_state = None;
        }
    }

    fn begin_local_game(&mut self, human_color: Color) {
        // A puzzle left open must not judge this game's moves.
        self.exit_puzzle_trainer();
        let time_label = self.local_time_preset.label();
        let difficulty_label = self.local_difficulty.label();
        let level = self.local_difficulty.numeric_level(self.adaptive_level);
        let config = crate::strength::StrengthConfig::from_level(level);
        {
            let mut state = self.state.lock().unwrap();
            state.board = Board::startpos();
            state.move_history.clear();
            state.local_history.clear();
            state.search_info = SearchInfo::default();
            state.local_search_generation = state.local_search_generation.wrapping_add(1);
            state.local_game = LocalGameState::new(
                human_color,
                self.local_time_preset,
                self.local_difficulty,
                level,
            );
            state.pending_animations.clear();
            state.game_saved = false;
            // Configure searcher for this difficulty level
            {
                let mut searcher = state.persistent_searcher.lock().unwrap();
                searcher.tt.clear();
                searcher.use_nnue = config.use_nnue
                    && crate::nnue::network::get_network().is_some();
                searcher.eval_noise = config.eval_noise_cp;
            }
            seed_local_history(&mut state);
            state.status_message = format!(
                "Local game started. You play {}. Time control: {}. Difficulty: {} (Level {}, ~{} Elo).",
                color_name(human_color),
                time_label,
                difficulty_label,
                level,
                crate::strength::estimated_elo(level),
            );
        }

        self.selected_square = None;
        self.drag_state = None;
        self.move_animations.clear();
        self.pending_promotion = None;
        self.flipped = human_color == Color::Black;
        // Remember this setup for next time — saved on actually starting a
        // game, not on every radio/dropdown click, so browsing options
        // without starting anything doesn't touch disk.
        self.save_settings("begin_local_game");

        if human_color == Color::Black {
            self.start_engine_search();
        }
    }

    fn navigate_local_history(&mut self, target_index: usize) {
        let history_label;

        {
            let mut state = self.state.lock().unwrap();
            if !state.local_game.active || target_index >= state.local_history.len() {
                return;
            }

            invalidate_local_search(&mut state, true);
            state.local_history_cursor = target_index;

            let Some(snapshot) = state.local_history.get(target_index).cloned() else {
                return;
            };

            restore_local_snapshot(&mut state, &snapshot);
            state.local_game.outcome = None;
            sync_local_move_history(&mut state);

            update_local_game_outcome(&mut state);

            history_label = local_history_label(target_index, snapshot.move_uci.as_deref());
            if let Some(outcome) = state.local_game.outcome {
                state.status_message = local_outcome_status(outcome, state.local_game.human_color);
            } else {
                state.status_message = format!("Reviewing {history_label}. Use the arrows or resume from here.");
            }
        }

        self.pending_promotion = None;
        self.selected_square = None;
        self.drag_state = None;
    }

    fn resume_local_game(&mut self) {
        let mut should_start_engine = false;

        {
            let mut state = self.state.lock().unwrap();
            if !state.local_game.active || state.local_game.outcome.is_some() {
                return;
            }

            state.local_game.time_control.start_turn_now();
            if state.board.side_to_move == state.local_game.human_color {
                state.status_message = "Resumed local game. Your turn.".to_string();
            } else {
                state.status_message = "Resumed local game. Focalors is thinking...".to_string();
                should_start_engine = true;
            }
        }

        if should_start_engine {
            self.start_engine_search();
        }
    }

    fn abort_local_game(&mut self) {
        let local_game = {
            let mut state = self.state.lock().unwrap();
            if !state.local_game.active {
                return;
            }

            invalidate_local_search(&mut state, true);
            let local_game = LocalGameState::idle(
                state.local_game.human_color,
                self.local_time_preset,
                self.local_difficulty,
                state.local_game.numeric_level,
            );
            state.board = Board::startpos();
            state.move_history.clear();
            state.local_history.clear();
            state.local_history_cursor = 0;
            state.local_game = local_game.clone();
            state.pending_animations.clear();
            state.status_message = "Local game ended. Back on the home screen.".to_string();
            local_game
        };

        self.pending_promotion = None;
        self.selected_square = None;
        self.drag_state = None;
        self.move_animations.clear();
        self.flipped = local_game.human_color == Color::Black;
    }

}

impl eframe::App for FocalorsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.update_local_clock_state();
        self.poll_system_theme(&ctx);

        // Auto-save completed games to DB
        {
            let should_save = {
                let state = self.state.lock().unwrap();
                state.local_game.active
                    && state.local_game.outcome.is_some()
                    && !state.game_saved
            };
            if should_save {
                self.save_completed_game();
                self.state.lock().unwrap().game_saved = true;
            }
        }

        let (show_home_screen, mode_label, searching) = {
            let state = self.state.lock().unwrap();
            let show_home_screen = !state.local_game.active;
            let mode_label = if show_home_screen { "Home" } else { "Local Play" };
            (show_home_screen, mode_label, state.search_info.searching)
        };

        // Request repaint periodically for live updates — but not several
        // times a second while the window isn't even focused. There's no
        // reason to wake up 5-10x/sec for a turn-based game nobody is
        // looking at.
        let focused = ctx.input(|i| i.focused);
        let repaint_after = if !focused {
            2000
        } else if self.has_running_local_clock() {
            100
        } else {
            200
        };
        ctx.request_repaint_after(Duration::from_millis(repaint_after));

        // Welcome dialog (first launch) — skip all other content
        if self.show_welcome {
            egui::CentralPanel::default().show_inside(ui, |_ui| {});
            self.draw_welcome_dialog(&ctx);
            return;
        }

        // Top menu bar
        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label(
                    hydra_heading("FOCALORS", 18.0)
                        .color(hydra_accent()),
                );
                ui.label(
                    egui::RichText::new(mode_label)
                        .size(12.0)
                        .color(hydra_subtle_text()),
                );
                if searching {
                    ui.separator();
                    ui.spinner();
                    hydra_badge(ui, "Thinking", hydra_warning());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggle_label = match self.ui_theme {
                        UiTheme::Dark => "Light Mode",
                        UiTheme::Light => "Dark Mode",
                        UiTheme::System if current_theme() == UiTheme::Dark => "Light Mode",
                        UiTheme::System => "Dark Mode",
                    };
                    if ui.add(theme_toggle_button(toggle_label)).clicked() {
                        self.set_ui_theme(&ctx, self.ui_theme.toggle());
                    }
                    if ui.add(theme_toggle_button("Settings")).clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
            });
        });

        // Settings window: opened from the top bar, so it works on every
        // screen and a board change shows live on the board behind it.
        self.draw_settings_window(&ctx);

        // Right panel: settings and controls (only during play)
        if !show_home_screen {
            egui::Panel::right("controls")
                .min_size(300.0)
                .max_size(360.0)
                .show_inside(ui, |ui| {
                    self.draw_controls(ui);
                });

            // Bottom info strip: clocks + status + last move (compact, no scroll)
            egui::Panel::bottom("play_status_bar")
                .resizable(false)
                .default_size(56.0)
                .min_size(48.0)
                .max_size(96.0)
                .show_inside(ui, |ui| {
                    self.draw_play_status_bar(ui);
                });
        }

        // Central panel: home (scrollable) or board only (fills remaining space)
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if show_home_screen {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Outer gutter: keeps dashboard content away from
                        // the window edge and the scrollbar so charts and
                        // bars don't run flush against the right side.
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 24,
                                right: 24,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                self.draw_idle_home(ui);
                            });
                    });
            } else {
                ui.add_space(6.0);
                self.draw_board(ui, None);
            }
        });
    }
}

// ── Launch the GUI ─────────────────────────────────────────────────────────

pub fn run_gui() {
    attacks::init();
    // Initialize NNUE (will use embedded net or fall back to HCE)
    let _ = crate::nnue::init(None);

    // Window and taskbar icon (Windows, X11). Wayland ignores it and takes
    // the icon from a desktop entry matched by the app id instead.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([950.0, 680.0])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Focalors Chess Engine")
        .with_app_id("focalors");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        // Explicit OpenGL backend — avoids wgpu/Vulkan compatibility issues
        // on some Linux setups while being plenty fast for our chess GUI.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Focalors",
        options,
        Box::new(|cc| Ok(Box::new(FocalorsApp::new(cc)))),
    )
    .expect("Failed to launch GUI");
}

/// The app icon for the window frame and taskbar, decoded from the embedded
/// 256px PNG. None if it ever fails to decode: the app then starts with
/// the default icon instead of refusing to start.
fn window_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../../assets/icon/focalors-256.png");
    match image::load_from_memory(PNG) {
        Ok(img) => {
            let img = img.into_rgba8();
            let (width, height) = img.dimensions();
            Some(egui::IconData { rgba: img.into_raw(), width, height })
        }
        Err(e) => {
            eprintln!("focalors: window icon failed to decode: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::uci_arrow_squares;
    use crate::types::Square;

    #[test]
    fn dark_mode_callout_fill_is_a_translucent_tint() {
        // Tests run on the startup theme (dark); make that assumption loud.
        assert!(
            matches!(super::current_theme(), super::UiTheme::Dark),
            "tests assume the startup theme (dark)"
        );
        let fill = super::hydra_callout_fill();
        // A valid premultiplied tint: translucent, no channel above alpha.
        // The additive accent that washed out the warning text had all
        // three channels far above its alpha.
        assert!(fill.a() > 0 && fill.a() < 255);
        assert!(fill.r() <= fill.a() && fill.g() <= fill.a() && fill.b() <= fill.a());
    }

    #[test]
    fn dark_mode_clock_row_keeps_the_shipped_fill() {
        assert!(
            matches!(super::current_theme(), super::UiTheme::Dark),
            "tests assume the startup theme (dark)"
        );
        assert_eq!(super::hydra_clock_active_fill(), super::hydra_accent_soft());
    }

    #[test]
    fn ending_a_puzzle_session_restores_the_start_position() {
        let fen = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
        let puzzle_board = crate::board::Board::from_fen(fen).expect("valid fen");
        let session = || {
            Some(super::PuzzleTrainerState {
                puzzle: crate::db::SavedPuzzle {
                    id: 1,
                    fen: fen.to_string(),
                    solution: "f1c4".to_string(),
                    theme: None,
                    rating: None,
                },
                board: puzzle_board.clone(),
                solution_move: crate::moves::Move::NULL,
                user_attempted: false,
                user_solved: false,
                show_hint: false,
                show_answer: false,
                wrong_attempts: 0,
            })
        };
        let start_fen = crate::board::Board::startpos().to_fen();

        // No game running: the board goes back to the start position.
        let mut state = super::SharedState::default();
        state.board = puzzle_board.clone();
        let mut trainer = session();
        let mut message = Some(("hint".to_string(), egui::Color32::WHITE, std::time::Instant::now()));
        assert!(super::end_puzzle_session(&mut trainer, &mut message, &mut state));
        assert!(trainer.is_none());
        assert!(message.is_none());
        assert_eq!(state.board.to_fen(), start_fen);
        // Nothing left to end.
        assert!(!super::end_puzzle_session(&mut trainer, &mut message, &mut state));

        // A game owns the board: the session ends, the board stays.
        let mut state = super::SharedState::default();
        state.local_game.active = true;
        state.board = puzzle_board.clone();
        let mut trainer = session();
        let mut message = None;
        assert!(super::end_puzzle_session(&mut trainer, &mut message, &mut state));
        assert!(trainer.is_none());
        assert_eq!(state.board.to_fen(), fen);
    }

    #[test]
    fn window_icon_is_a_square_rgba_image() {
        let icon = super::window_icon().expect("embedded icon must decode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Not a blank canvas: some pixel is opaque.
        assert!(icon.rgba.chunks(4).any(|px| px[3] == 255));
    }

    #[test]
    fn board_theme_round_trips_and_walnut_is_the_fallback() {
        use super::BoardTheme;

        for (i, theme) in BoardTheme::ALL.into_iter().enumerate() {
            assert_eq!(theme.index() as usize, i, "ALL must follow declaration order");
            assert_eq!(BoardTheme::from_index(theme.index()), theme);
            assert_eq!(BoardTheme::from_str(theme.as_str()), theme);
        }
        // A corrupted row, or a preset written by a newer version, falls
        // back to the default board instead of failing.
        assert_eq!(BoardTheme::from_str("neon"), BoardTheme::Walnut);
        assert_eq!(BoardTheme::from_index(200), BoardTheme::Walnut);

        // The default board keeps the exact colors it had before themes.
        let walnut = BoardTheme::Walnut.palette();
        assert_eq!(walnut.light, egui::Color32::from_rgb(227, 218, 201));
        assert_eq!(walnut.dark, egui::Color32::from_rgb(120, 99, 81));
        assert_eq!(walnut.legal, egui::Color32::from_rgb(168, 149, 126));

        // Every preset's legal-move tone must stay distinct from both of its
        // squares and from the gold highlights, or legal squares vanish.
        for theme in BoardTheme::ALL {
            let p = theme.palette();
            for other in [p.light, p.dark, super::BOARD_SELECTED, super::BOARD_LAST_MOVE] {
                assert_ne!(p.legal, other, "{theme:?}");
            }
        }
    }

    /// Tessellates the arrow fill the way the GUI does and samples the
    /// resulting triangles: every point well inside the arrow must be covered
    /// exactly once at full strength, every point well outside not at all.
    /// A fan started from the wrong corner, a reversed winding, or shaft and
    /// head drawn as overlapping pieces would all fail here.
    #[test]
    fn arrow_fill_covers_the_arrow_exactly_once() {
        use egui::epaint::{Mesh, TessellationOptions, Tessellator};
        use egui::{pos2, Color32, Pos2};

        let sq = 100.0_f32;
        let center = |file: f32, rank: f32| pos2((file + 0.5) * sq, (rank + 0.5) * sq);
        // (from, to) as (file, rank) pairs: every direction class an arrow
        // takes on a board, including the short one-square and knight hops.
        let arrows = [
            ((4.0, 6.0), (4.0, 4.0)), // up
            ((4.0, 1.0), (4.0, 3.0)), // down
            ((0.0, 0.0), (7.0, 0.0)), // right, full width
            ((7.0, 7.0), (0.0, 7.0)), // left
            ((6.0, 6.0), (2.0, 2.0)), // long diagonal
            ((1.0, 2.0), (5.0, 6.0)), // the other diagonal direction
            ((6.0, 0.0), (5.0, 2.0)), // knight
            ((1.0, 7.0), (2.0, 5.0)), // knight, mirrored
            ((3.0, 3.0), (4.0, 3.0)), // one square
            ((3.0, 3.0), (2.0, 4.0)), // one square, diagonal
        ];

        // Signed-area style edge function and helpers for the sampling.
        let edge = |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let dist_to_segment = |a: Pos2, b: Pos2, p: Pos2| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0);
            (p - (a + ab * t)).length()
        };
        let inside_polygon = |poly: &[Pos2], p: Pos2| {
            let mut inside = false;
            let mut j = poly.len() - 1;
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[j]);
                if (a.y > p.y) != (b.y > p.y)
                    && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                {
                    inside = !inside;
                }
                j = i;
            }
            inside
        };

        for ((ff, fr), (tf, tr)) in arrows {
            let outline = super::annotation_arrow_outline(center(ff, fr), center(tf, tr), sq)
                .expect("distinct squares give an arrow");
            assert_eq!(outline.len(), 7);

            let mut tessellator =
                Tessellator::new(1.0, TessellationOptions::default(), [1, 1], Vec::new());
            let mut mesh = Mesh::default();
            tessellator.tessellate_shape(
                egui::Shape::convex_polygon(outline.clone(), Color32::WHITE, egui::Stroke::NONE),
                &mut mesh,
            );
            assert!(!mesh.indices.is_empty(), "arrow produced no triangles");

            // Coverage at a point: summed vertex-alpha of every triangle
            // containing it. 1.0 means painted once at full strength.
            let coverage = |p: Pos2| -> f32 {
                let mut total = 0.0;
                for tri in mesh.indices.chunks_exact(3) {
                    let [a, b, c] = [
                        mesh.vertices[tri[0] as usize],
                        mesh.vertices[tri[1] as usize],
                        mesh.vertices[tri[2] as usize],
                    ];
                    let area = edge(a.pos, b.pos, c.pos);
                    if area.abs() < 1e-6 {
                        continue;
                    }
                    let (wa, wb, wc) = (
                        edge(b.pos, c.pos, p) / area,
                        edge(c.pos, a.pos, p) / area,
                        edge(a.pos, b.pos, p) / area,
                    );
                    if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
                        total += (wa * a.color.a() as f32
                            + wb * b.color.a() as f32
                            + wc * c.color.a() as f32)
                            / 255.0;
                    }
                }
                total
            };

            let (mut checked_inside, mut checked_outside) = (0, 0);
            let mut y = 1.3_f32;
            while y < 8.0 * sq {
                let mut x = 0.9_f32;
                while x < 8.0 * sq {
                    let p = pos2(x, y);
                    let edge_distance = (0..outline.len())
                        .map(|i| dist_to_segment(outline[i], outline[(i + 1) % outline.len()], p))
                        .fold(f32::INFINITY, f32::min);
                    // Skip the anti-aliasing band along the edges.
                    if edge_distance > 2.0 {
                        let c = coverage(p);
                        if inside_polygon(&outline, p) {
                            assert!(
                                (c - 1.0).abs() < 0.02,
                                "inside point {p:?} has coverage {c} for arrow {ff},{fr} -> {tf},{tr}"
                            );
                            checked_inside += 1;
                        } else {
                            assert!(
                                c < 0.02,
                                "outside point {p:?} has coverage {c} for arrow {ff},{fr} -> {tf},{tr}"
                            );
                            checked_outside += 1;
                        }
                    }
                    x += 3.7;
                }
                y += 3.7;
            }
            assert!(checked_inside > 50, "too few inside samples: {checked_inside}");
            assert!(checked_outside > 1000);
        }

        // Same point twice: nothing to draw.
        assert!(super::annotation_arrow_outline(center(3.0, 3.0), center(3.0, 3.0), sq).is_none());
    }

    #[test]
    fn settings_preview_position_matches_its_last_move_squares() {
        use crate::types::{Color, Piece};

        let board = crate::board::Board::from_fen(super::SETTINGS_PREVIEW_FEN)
            .expect("preview FEN must parse");
        let [from, to] = super::SETTINGS_PREVIEW_LAST_MOVE;
        assert_eq!(from, Square::from_algebraic("g8").unwrap().0);
        assert_eq!(to, Square::from_algebraic("f6").unwrap().0);
        assert_eq!(board.piece_on(Square(from)), None);
        assert_eq!(board.piece_on(Square(to)), Some((Color::Black, Piece::Knight)));
    }

    #[test]
    fn piece_set_round_trips_and_cburnett_is_the_fallback() {
        use super::PieceSet;

        for set in PieceSet::ALL {
            assert_eq!(PieceSet::from_str(set.as_str()), set);
        }
        assert_eq!(PieceSet::from_str("sadsnake"), PieceSet::Cburnett);
        assert_eq!(PieceSet::from_str(""), PieceSet::Cburnett);
    }

    /// Every set embeds the same twelve pieces in the same order, each a
    /// 128px square PNG, so the board draws any set identically.
    #[test]
    fn every_piece_set_embeds_twelve_square_images() {
        use crate::types::{Color, Piece};

        let expected: Vec<(Color, Piece)> = [Color::White, Color::Black]
            .into_iter()
            .flat_map(|color| {
                [Piece::King, Piece::Queen, Piece::Rook, Piece::Bishop, Piece::Knight, Piece::Pawn]
                    .into_iter()
                    .map(move |piece| (color, piece))
            })
            .collect();
        for set in super::PieceSet::ALL {
            let got: Vec<(Color, Piece)> =
                set.images().iter().map(|&(color, piece, _, _)| (color, piece)).collect();
            assert_eq!(got, expected, "{set:?}: image table order");
            for &(color, _, name, data) in set.images() {
                let prefix = match color {
                    Color::White => 'w',
                    Color::Black => 'b',
                };
                assert!(name.starts_with(prefix), "{set:?}/{name}: wrong side prefix");
                let img = image::load_from_memory(data)
                    .unwrap_or_else(|e| panic!("{set:?}/{name}: {e}"));
                assert_eq!(
                    (img.width(), img.height()),
                    (128, 128),
                    "{set:?}/{name} must be a 128px square"
                );
            }
        }
    }

    #[test]
    fn analysis_quality_round_trips_and_standard_is_the_old_default() {
        use super::AnalysisQuality;

        for quality in AnalysisQuality::ALL {
            assert_eq!(AnalysisQuality::from_str(quality.as_str()), quality);
        }
        assert_eq!(AnalysisQuality::from_str("ultra"), AnalysisQuality::Standard);
        assert_eq!(AnalysisQuality::from_str(""), AnalysisQuality::Standard);
        // Every review before the choice existed ran at depth 14.
        assert_eq!(AnalysisQuality::Standard.depth(), 14);
        assert!(AnalysisQuality::Quick.depth() < AnalysisQuality::Standard.depth());
        assert!(AnalysisQuality::Standard.depth() < AnalysisQuality::Deep.depth());
    }

    #[test]
    fn stored_custom_opponent_values_are_clamped_into_the_slider_ranges() {
        use super::EngineSettings;

        let s = EngineSettings::from_stored(false, 5000, 12);
        assert_eq!(
            s,
            EngineSettings { max_depth: 12, think_time_ms: 5000, use_time_limit: false }
        );
        // A hand-edited or corrupted row cannot yield a zero-depth or an
        // hour-long search.
        let s = EngineSettings::from_stored(true, -1, 0);
        assert_eq!((s.think_time_ms, s.max_depth), (500, 1));
        let s = EngineSettings::from_stored(true, 3_600_000, 99);
        assert_eq!((s.think_time_ms, s.max_depth), (30_000, 30));
        // The defaults sit inside the ranges, so a fresh row round-trips.
        let d = EngineSettings::default();
        assert_eq!(
            EngineSettings::from_stored(d.use_time_limit, d.think_time_ms as i64, d.max_depth as i64),
            d
        );
    }

    #[test]
    fn pending_animation_derives_piece_and_castling_rook() {
        crate::attacks::init();
        let board = crate::board::Board::startpos();

        // Normal move: the moving piece, no rook companion.
        let e2e4 = crate::moves::Move::new(
            Square::from_algebraic("e2").unwrap(),
            Square::from_algebraic("e4").unwrap(),
        );
        let anim = super::pending_animation(&board, e2e4, true).unwrap();
        assert_eq!(anim.piece, (crate::types::Color::White, crate::types::Piece::Pawn));
        assert_eq!(anim.rook, None);
        assert!(!anim.captured, "pushing a pawn to an empty square is not a capture");
        assert!(anim.animate);

        // A drag-drop still carries `captured` for the sound decision, it
        // just shouldn't glide (the piece is already on its destination
        // square on screen).
        assert!(!super::pending_animation(&board, e2e4, false).unwrap().animate);

        // Castling arithmetic (pending_animation doesn't re-check legality;
        // callers only hand it moves that were actually applied). Kings sit
        // on e1/e8 in the start position, so all four castles resolve.
        let castle = |from: &str, to: &str| {
            crate::moves::Move::new_castling(
                Square::from_algebraic(from).unwrap(),
                Square::from_algebraic(to).unwrap(),
            )
        };
        let rook_path = |mv| super::pending_animation(&board, mv, true).unwrap().rook.unwrap();
        assert_eq!(rook_path(castle("e1", "g1")), (7, 5)); // h1 -> f1
        assert_eq!(rook_path(castle("e1", "c1")), (0, 3)); // a1 -> d1
        assert_eq!(rook_path(castle("e8", "g8")), (63, 61)); // h8 -> f8
        assert_eq!(rook_path(castle("e8", "c8")), (56, 59)); // a8 -> d8

        // Null move never animates.
        assert!(super::pending_animation(&board, crate::moves::Move::NULL, true).is_none());
    }

    #[test]
    fn live_board_highlights_the_move_behind_the_shown_snapshot() {
        let mut state = super::SharedState::default();
        // No local game: nothing to highlight, whatever the history says.
        assert_eq!(super::live_last_move_squares(&state), None);
        state.local_game.active = true;
        assert_eq!(super::live_last_move_squares(&state), None, "start position");

        super::record_local_snapshot(&mut state, "e2e4".to_string());
        super::record_local_snapshot(&mut state, "e7e5".to_string());
        let sq = |name: &str| Square::from_algebraic(name).unwrap().0;
        assert_eq!(super::live_last_move_squares(&state), Some((sq("e7"), sq("e5"))));
        // Stepping back through the history shows the move that led there.
        state.local_history_cursor = 1;
        assert_eq!(super::live_last_move_squares(&state), Some((sq("e2"), sq("e4"))));
        state.local_history_cursor = 0;
        assert_eq!(super::live_last_move_squares(&state), None);
    }

    #[test]
    fn uci_arrow_squares_parses_plain_and_promotion() {
        // e2e4: from e2 (file 4, rank 1 -> idx 12), to e4 (idx 28).
        assert_eq!(
            uci_arrow_squares("e2e4"),
            Some((Square::from_algebraic("e2").unwrap().0, Square::from_algebraic("e4").unwrap().0))
        );
        // Promotion suffix is ignored; only the from/to squares matter.
        assert_eq!(
            uci_arrow_squares("e7e8q"),
            Some((Square::from_algebraic("e7").unwrap().0, Square::from_algebraic("e8").unwrap().0))
        );
    }

    #[test]
    fn uci_arrow_squares_rejects_malformed() {
        assert_eq!(uci_arrow_squares(""), None);
        assert_eq!(uci_arrow_squares("e2"), None); // too short
        assert_eq!(uci_arrow_squares("z9z9"), None); // off-board
    }

    #[test]
    fn format_eval_normal_scores_are_signed_pawns() {
        assert_eq!(super::format_eval(0), "+0.00");
        assert_eq!(super::format_eval(217), "+2.17");
        assert_eq!(super::format_eval(-135), "-1.35");
    }

    #[test]
    fn format_eval_mate_scores_render_as_mate_in_n() {
        // MATE_SCORE is 29000; 28997 => mate in (29000-28997+1)/2 = 2.
        assert_eq!(super::format_eval(28997), "M2");
        assert_eq!(super::format_eval(-28997), "-M2");
        // Just below the cutoff stays a pawn count, not a mate.
        assert_eq!(super::format_eval(19999), "+199.99");
    }
}
