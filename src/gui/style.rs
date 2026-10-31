use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiTheme {
    Dark,
    Light,
    /// Follows the OS's light/dark signal. On Omarchy specifically, also
    /// adopts Omarchy's actual accent/background palette instead of just
    /// picking between Focalors' own built-in Dark/Light looks — see
    /// `gui/omarchy.rs` and `resolve_system_theme`.
    System,
}

impl UiTheme {
    /// Used by the quick top-bar toggle, which is a plain two-way switch:
    /// clicking it while on `System` overrides to the opposite of
    /// whatever System is *currently* resolving to — not a hardcoded
    /// guess, since System showing as effectively dark and "toggling" to
    /// dark again would be a no-op the button didn't advertise.
    pub(crate) fn toggle(self) -> Self {
        match self {
            UiTheme::Dark => UiTheme::Light,
            UiTheme::Light => UiTheme::Dark,
            UiTheme::System => current_theme().toggle(),
        }
    }

    /// Only ever called with a *resolved* Dark/Light (never `System` —
    /// callers resolve first), so the `System` arm is just a defensive
    /// default and never actually reached in practice.
    pub(crate) fn index(self) -> u8 {
        match self {
            UiTheme::Dark | UiTheme::System => 0,
            UiTheme::Light => 1,
        }
    }

    /// String form persisted to settings.toml.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            UiTheme::Dark => "dark",
            UiTheme::Light => "light",
            UiTheme::System => "system",
        }
    }

    /// Inverse of `as_str`. Unknown values fall back to Light so a
    /// corrupted or older settings file doesn't break startup.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "dark" => UiTheme::Dark,
            "system" => UiTheme::System,
            _ => UiTheme::Light,
        }
    }
}

pub(crate) fn current_theme() -> UiTheme {
    match ACTIVE_THEME.load(Ordering::Relaxed) {
        1 => UiTheme::Light,
        _ => UiTheme::Dark,
    }
}

/// The Omarchy-derived chrome palette, when `UiTheme::System` resolved to
/// one. Cheap: a `Mutex` lock and a `Copy`.
fn active_omarchy_palette() -> Option<OmarchyPalette> {
    *ACTIVE_OMARCHY_PALETTE.lock().unwrap()
}

pub(crate) fn theme_rgb(dark: [u8; 3], light: [u8; 3]) -> egui::Color32 {
    // current_theme() is always a resolved Dark/Light (System is resolved
    // away before it's ever stored), but the type itself still has three
    // variants, so this reads as a bool rather than an inexhaustive match.
    if current_theme() == UiTheme::Dark {
        egui::Color32::from_rgb(dark[0], dark[1], dark[2])
    } else {
        egui::Color32::from_rgb(light[0], light[1], light[2])
    }
}

pub(crate) fn theme_rgba(dark: [u8; 4], light: [u8; 4]) -> egui::Color32 {
    if current_theme() == UiTheme::Dark {
        egui::Color32::from_rgba_premultiplied(dark[0], dark[1], dark[2], dark[3])
    } else {
        egui::Color32::from_rgba_premultiplied(light[0], light[1], light[2], light[3])
    }
}

pub(crate) fn hydra_accent() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.accent;
    }
    theme_rgb([118, 147, 255], [69, 102, 214])
}

/// "Soft" accent behind selected nav buttons and the running clock row.
/// The values are unmultiplied but the constructor is premultiplied, so it
/// renders as a near-solid accent in dark mode and as white in light mode.
/// That look shipped and those widgets are tuned for it; do not switch the
/// constructor without re-checking every user. Callouts use
/// `hydra_callout_fill()`, which is a real tint.
pub(crate) fn hydra_accent_soft() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.accent_soft;
    }
    theme_rgba([118, 147, 255, 34], [69, 102, 214, 24])
}

/// Fill behind the running clock row. Dark mode keeps the solid accent it
/// shipped with; light mode gets a faint tint, because the same color clamps
/// to white there and the running side was only told apart by text shade.
pub(crate) fn hydra_clock_active_fill() -> egui::Color32 {
    if current_theme() == UiTheme::Dark {
        hydra_accent_soft()
    } else {
        egui::Color32::from_rgba_unmultiplied(69, 102, 214, 28)
    }
}

/// Fill of the callout boxes (the paused-history notice, the auto-adjust
/// message). Dark mode gets a faint accent tint so warning or success text
/// keeps its contrast; light mode keeps the plain card fill, where the
/// accent stroke alone marks the box.
pub(crate) fn hydra_callout_fill() -> egui::Color32 {
    if current_theme() == UiTheme::Dark {
        egui::Color32::from_rgba_unmultiplied(118, 147, 255, 34)
    } else {
        hydra_panel_fill()
    }
}

pub(crate) fn hydra_text() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.text;
    }
    theme_rgb([232, 236, 242], [34, 39, 46])
}

pub(crate) fn hydra_text_on_accent() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.text_on_accent;
    }
    theme_rgb([246, 248, 252], [247, 249, 255])
}

pub(crate) fn hydra_subtle_text() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.subtle_text;
    }
    theme_rgb([151, 160, 173], [82, 92, 108])
}

pub(crate) fn hydra_bg() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.bg;
    }
    theme_rgb([20, 24, 31], [237, 240, 244])
}

pub(crate) fn hydra_panel_fill() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.panel;
    }
    // Slightly brighter than the page background so borderless cards still
    // read as "elevated" surfaces. With the 1px border removed, this fill
    // delta is the primary differentiator between card and page.
    theme_rgb([36, 41, 50], [252, 253, 255])
}

pub(crate) fn hydra_panel_alt_fill() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.panel_alt;
    }
    theme_rgb([38, 44, 53], [242, 245, 248])
}

pub(crate) fn hydra_panel_raised_fill() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.panel_raised;
    }
    theme_rgb([47, 55, 65], [233, 237, 242])
}

pub(crate) fn hydra_border() -> egui::Color32 {
    if let Some(p) = active_omarchy_palette() {
        return p.border;
    }
    theme_rgb([66, 76, 89], [208, 215, 224])
}

pub(crate) fn hydra_success() -> egui::Color32 {
    theme_rgb([111, 191, 143], [51, 126, 87])
}

pub(crate) fn hydra_warning() -> egui::Color32 {
    theme_rgb([203, 159, 92], [170, 121, 51])
}

pub(crate) fn hydra_danger() -> egui::Color32 {
    theme_rgb([199, 109, 102], [181, 84, 72])
}

// Chess move classification palette — used by the Game Review move list,
// the Progress error breakdown bars, and any eval-graph highlights. Matches
// the chess.com convention so users coming from that tool already recognize
// the meaning of each color. Allow dead code until per-tab rebuilds wire
// them in.

// Chess move classification palette — used by the Game Review move list,
// the Progress error breakdown bars, and any eval-graph highlights. Matches
// the chess.com convention so users coming from that tool already recognize
// the meaning of each color. Allow dead code until per-tab rebuilds wire
// them in.
pub(crate) fn class_book() -> egui::Color32 {
    theme_rgb([170, 130, 90], [140, 100, 60])
}

#[allow(dead_code)]
pub(crate) fn class_best() -> egui::Color32 {
    theme_rgb([120, 200, 120], [70, 160, 70])
}

#[allow(dead_code)]
pub(crate) fn class_good() -> egui::Color32 {
    theme_rgb([110, 180, 220], [60, 140, 200])
}

#[allow(dead_code)]
pub(crate) fn class_inaccuracy() -> egui::Color32 {
    theme_rgb([230, 200, 100], [200, 160, 50])
}

#[allow(dead_code)]
pub(crate) fn class_mistake() -> egui::Color32 {
    theme_rgb([230, 150, 80], [200, 110, 40])
}

#[allow(dead_code)]
pub(crate) fn class_blunder() -> egui::Color32 {
    theme_rgb([220, 100, 100], [190, 70, 70])
}

#[allow(dead_code)]
pub(crate) fn class_brilliant() -> egui::Color32 {
    theme_rgb([100, 220, 220], [50, 180, 180])
}

/// Embed a tiny trend line inside a KPI tile. No axes, no grid, no
/// interaction — pure visual cue showing direction of a series. Pass an
/// `id` that's unique among siblings to keep egui's plot bookkeeping happy.
#[allow(dead_code)]
pub(crate) fn sparkline(ui: &mut egui::Ui, id: &str, points: &[f64], color: egui::Color32, height: f32) {
    if points.len() < 2 {
        ui.add_space(height);
        return;
    }
    let pts: Vec<[f64; 2]> = points
        .iter()
        .enumerate()
        .map(|(i, v)| [i as f64, *v])
        .collect();
    let line = egui_plot::Line::new(id, egui_plot::PlotPoints::new(pts)).color(color);
    egui_plot::Plot::new(format!("sparkline_{id}"))
        .height(height)
        .show_axes(false)
        .show_grid(false)
        .show_x(false)
        .show_y(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .show_background(false)
        .show(ui, |plot_ui| {
            plot_ui.line(line);
        });
}

/// Draw a circular accuracy/progress gauge — a partial ring with the
/// percentage rendered as text in the center. Used in the Progress tab as
/// the headline accuracy widget. `size` controls overall diameter; the
/// stroke and font sizes scale with it.
pub(crate) fn draw_radial_gauge(ui: &mut egui::Ui, size: f32, percentage: f64, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let center = rect.center();
    let stroke_w = (size * 0.06).max(6.0);
    let radius = size / 2.0 - stroke_w - 2.0;
    let painter = ui.painter();

    // Background ring — uses hydra_border so it has enough contrast
    // against both the dark and the light page bg. hydra_panel_alt_fill
    // was too close to the light-mode bg and the ring disappeared.
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(stroke_w, hydra_border()),
    );

    // Foreground arc — starts at the top (12 o'clock), sweeps clockwise.
    let pct = (percentage / 100.0).clamp(0.0, 1.0);
    if pct > 0.0 {
        const SEGMENTS: usize = 64;
        let n = ((pct * SEGMENTS as f64).round() as usize).max(1);
        for i in 0..n {
            let t1 = i as f32 / SEGMENTS as f32;
            let t2 = (i + 1) as f32 / SEGMENTS as f32;
            let a1 = -std::f32::consts::FRAC_PI_2 + t1 * 2.0 * std::f32::consts::PI;
            let a2 = -std::f32::consts::FRAC_PI_2 + t2 * 2.0 * std::f32::consts::PI;
            let p1 = egui::pos2(
                center.x + radius * a1.cos(),
                center.y + radius * a1.sin(),
            );
            let p2 = egui::pos2(
                center.x + radius * a2.cos(),
                center.y + radius * a2.sin(),
            );
            painter.line_segment([p1, p2], egui::Stroke::new(stroke_w, color));
        }
    }

    // Center label — big percentage + small "accuracy" subtitle.
    painter.text(
        center + egui::vec2(0.0, -8.0),
        egui::Align2::CENTER_CENTER,
        format!("{percentage:.1}%"),
        egui::FontId::proportional(size * 0.18),
        hydra_text(),
    );
    painter.text(
        center + egui::vec2(0.0, size * 0.12),
        egui::Align2::CENTER_CENTER,
        "accuracy",
        egui::FontId::proportional(size * 0.075),
        hydra_subtle_text(),
    );
}

/// Draw a labeled W/L/D stacked horizontal bar. Used in the Statistics
/// Results card to visualize overall + by-color + by-time-control results
/// as a single comparable widget rather than text rows.
pub(crate) fn draw_result_bar(ui: &mut egui::Ui, label: &str, w: u32, l: u32, d: u32) {
    let total = w + l + d;
    if total == 0 {
        return;
    }
    let w_pct = w as f32 / total as f32;
    let d_pct = d as f32 / total as f32;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{:.0}% win", w as f64 / total as f64 * 100.0))
                    .size(10.0)
                    .color(hydra_subtle_text()),
            );
        });
    });
    let bar_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, 10.0), egui::Sense::hover());
    let p = ui.painter();
    // Background (covers full row, gets overpainted left-to-right).
    p.rect_filled(rect, 5.0, hydra_panel_alt_fill());
    let w_w = bar_w * w_pct;
    let d_w = bar_w * d_pct;
    if w_w > 0.0 {
        let r = egui::Rect::from_min_size(rect.min, egui::vec2(w_w, 10.0));
        p.rect_filled(r, 5.0, hydra_success());
    }
    if d_w > 0.0 {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + w_w, rect.min.y),
            egui::vec2(d_w, 10.0),
        );
        p.rect_filled(r, 0.0, hydra_subtle_text());
    }
    let l_w = bar_w - w_w - d_w;
    if l_w > 0.0 {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + w_w + d_w, rect.min.y),
            egui::vec2(l_w, 10.0),
        );
        p.rect_filled(r, 5.0, hydra_danger());
    }
    ui.label(
        egui::RichText::new(format!("{w}W  {d}D  {l}L"))
            .size(10.0)
            .color(hydra_subtle_text()),
    );
}

// ── Layout tokens ────────────────────────────────────────────────────────
/// Gap between cards laid out in a row or column grid.
pub(crate) const CARD_GAP: f32 = 16.0;
/// Vertical rhythm between page sections.
pub(crate) const SECTION_GAP: f32 = 16.0;
/// Minimum height of a KPI tile so a row of tiles lines up regardless of
/// which ones carry a delta.
pub(crate) const STAT_TILE_H: f32 = 84.0;
/// Minimum height of a chart/list card so both cards in a row share a
/// bottom edge regardless of content.
pub(crate) const ROW_CARD_H: f32 = 310.0;
/// Minimum height of the Overview's Recent Games / Your Openings cards.
pub(crate) const OVERVIEW_ROW_H: f32 = 280.0;
/// Progress row 1: the six-class Move Breakdown list sets the height, and
/// the accuracy gauge card matches it.
pub(crate) const PROGRESS_ROW_H: f32 = 356.0;
/// The openings list scrolls past this height so a long list cannot
/// stretch its card past its row partner.
pub(crate) const OPENINGS_LIST_MAX_H: f32 = 196.0;
/// Analyze page: both cards share this height; the games list scrolls
/// inside it and the PGN box grows to fill it.
pub(crate) const ANALYZE_ROW_H: f32 = 560.0;
/// Narrowest a History game card may get before the grid drops a column.
pub(crate) const HISTORY_TILE_MIN_W: f32 = 420.0;
/// Settings window: the board-theme swatches sit in a grid this many wide.
pub(crate) const SETTINGS_SWATCH_COLS: usize = 4;
/// Edge of one swatch's 4x4 color sample.
pub(crate) const SETTINGS_SWATCH: f32 = 76.0;
/// Room under the sample for the preset's name.
pub(crate) const SETTINGS_SWATCH_LABEL_H: f32 = 22.0;
pub(crate) const SETTINGS_SWATCH_GAP: f32 = 12.0;
/// The preview board is exactly two swatch rows tall, so it lines up with
/// the grid beside it.
pub(crate) const SETTINGS_PREVIEW_SIDE: f32 =
    2.0 * (SETTINGS_SWATCH + SETTINGS_SWATCH_LABEL_H) + SETTINGS_SWATCH_GAP;
pub(crate) const SETTINGS_PREVIEW_GAP: f32 = 20.0;
/// Custom opponent window: content width, enough for a slider with its
/// value box beside it.
pub(crate) const CUSTOM_OPPONENT_W: f32 = 360.0;
/// How long Resign and End Game stay armed after their first click.
pub(crate) const DANGER_CONFIRM_WINDOW: Duration = Duration::from_secs(4);
/// Swatch grid + gap + preview: fixes the window width so nothing reflows.
pub(crate) const SETTINGS_CONTENT_W: f32 = SETTINGS_SWATCH_COLS as f32 * SETTINGS_SWATCH
    + (SETTINGS_SWATCH_COLS as f32 - 1.0) * SETTINGS_SWATCH_GAP
    + SETTINGS_PREVIEW_GAP
    + SETTINGS_PREVIEW_SIDE;

/// One KPI tile: muted caps label, big value, optional colored delta
/// (`(is_up, text)`), muted sub-line. Fills its column width.
pub(crate) fn hydra_stat_tile(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    delta: Option<(bool, String)>,
    sub: &str,
) {
    hydra_card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(STAT_TILE_H);
        ui.label(hydra_heading(label, 10.0).color(hydra_subtle_text()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(hydra_heading(value, 26.0));
            if let Some((up, text)) = delta {
                let color = if up { hydra_success() } else { hydra_danger() };
                let arrow = if up { "▲" } else { "▼" };
                ui.label(
                    egui::RichText::new(format!("{arrow} {text}"))
                        .size(11.0)
                        .color(color)
                        .strong(),
                );
            }
        });
        ui.add_space(2.0);
        ui.label(egui::RichText::new(sub).size(11.0).color(hydra_subtle_text()));
    });
}

/// Inline chart legend: a colored dot and muted label per series.
pub(crate) fn chart_legend_row(ui: &mut egui::Ui, entries: &[(egui::Color32, &str)]) {
    // `horizontal` keeps the parent's direction, so inside a right-to-left
    // header the pieces are placed right-to-left too: emit them reversed
    // there so the dot still reads before its label and entries keep order.
    let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
    let dot = |ui: &mut egui::Ui, color: egui::Color32| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
    };
    let text = |ui: &mut egui::Ui, label: &str| {
        ui.label(egui::RichText::new(label).size(10.0).color(hydra_subtle_text()));
    };
    ui.horizontal(|ui| {
        let ordered: Vec<&(egui::Color32, &str)> = if rtl {
            entries.iter().rev().collect()
        } else {
            entries.iter().collect()
        };
        for (color, label) in ordered {
            if rtl {
                ui.add_space(6.0);
                text(ui, label);
                dot(ui, *color);
            } else {
                dot(ui, *color);
                text(ui, label);
                ui.add_space(6.0);
            }
        }
    });
}

/// A labelled fact without card chrome: muted caps label, value, muted
/// sub-line. For fact rows inside a card (no nested cards).
pub(crate) fn hydra_fact(ui: &mut egui::Ui, label: &str, value: &str, sub: &str) {
    ui.label(hydra_heading(label, 10.0).color(hydra_subtle_text()));
    ui.add_space(2.0);
    ui.label(hydra_heading(value, 20.0));
    ui.label(egui::RichText::new(sub).size(11.0).color(hydra_subtle_text()));
}

pub(crate) fn hydra_card_frame() -> egui::Frame {
    // Borderless cards: distinguished from the page background only by a
    // soft fill and a very faint shadow. Removing the 1px stroke is what
    // makes a multi-card page feel like one cohesive surface instead of a
    // grid of separate boxes — the original "window in window" complaint
    // came back through the border once we had many cards on a page.
    egui::Frame::new()
        .fill(hydra_panel_fill())
        .corner_radius(8)
        .inner_margin(egui::Margin::same(18))
        .shadow(egui::epaint::Shadow {
            offset: [0, 1],
            blur: 6,
            spread: 0,
            color: egui::Color32::from_black_alpha(10),
        })
}

pub(crate) fn hydra_callout_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(hydra_callout_fill())
        .stroke(egui::Stroke::new(1.0_f32, hydra_accent()))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(10))
}

pub(crate) fn hydra_badge(ui: &mut egui::Ui, label: &str, fill: egui::Color32) {
    let text_color = if fill == hydra_panel_alt_fill() || fill == hydra_panel_raised_fill() {
        hydra_subtle_text()
    } else {
        fill
    };
    ui.label(
        egui::RichText::new(label)
            .size(10.0)
            .strong()
            .color(text_color),
    );
}

pub(crate) fn home_nav_button<'a>(selected: bool, label: &'a str) -> egui::Button<'a> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(if selected { hydra_text() } else { hydra_subtle_text() }),
    )
    .fill(if selected {
        hydra_accent_soft()
    } else {
        egui::Color32::TRANSPARENT
    })
    .stroke(egui::Stroke::new(
        if selected { 1.0_f32 } else { 0.0_f32 },
        if selected { hydra_accent() } else { egui::Color32::TRANSPARENT },
    ))
    .corner_radius(0)
}

pub(crate) fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(13.0)
            .strong()
            .color(hydra_text_on_accent()),
    )
    .fill(hydra_accent())
    .stroke(egui::Stroke::new(1.0_f32, hydra_accent()))
    .corner_radius(2)
}

pub(crate) fn secondary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(hydra_text()),
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0_f32, hydra_border()))
    .corner_radius(2)
}

pub(crate) fn danger_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(hydra_danger()),
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0_f32, hydra_danger()))
    .corner_radius(2)
}

pub(crate) fn theme_toggle_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .strong()
            .color(hydra_text()),
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0_f32, hydra_border()))
    .corner_radius(2)
}

/// Embedded UI typeface: Inter (OFL, see assets/fonts/LICENSE-Inter.txt).
/// Regular becomes the proportional default. SemiBold is registered as its
/// own family for headings, because egui's `strong()` only changes the text
/// color and never the weight, so a weight hierarchy needs a second face.
pub(crate) const HEADING_FAMILY: &str = "InterSemiBold";

pub(crate) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        HEADING_FAMILY.to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    proportional.insert(0, "Inter".to_owned());
    // Headings: SemiBold first, then the same fallback chain (emoji, symbols).
    let mut heading = proportional.clone();
    heading.insert(0, HEADING_FAMILY.to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Name(HEADING_FAMILY.into()), heading);
    ctx.set_fonts(fonts);
}

pub(crate) fn hydra_heading_family() -> egui::FontFamily {
    egui::FontFamily::Name(HEADING_FAMILY.into())
}

/// Heading text: Inter SemiBold at `size` in the strong text color. Chain
/// `.color(..)` after it to override the color (e.g. muted KPI labels).
pub(crate) fn hydra_heading(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .family(hydra_heading_family())
        .strong()
}

/// Resolves `UiTheme::System` to a concrete `Dark`/`Light`, and sets or
/// clears `ACTIVE_OMARCHY_PALETTE` as a side effect — the one place that
/// decides whether "Match System" means "Omarchy's actual palette" or
/// "just follow the OS's light/dark signal" this frame. A no-op for an
/// already-concrete `Dark`/`Light` beyond clearing the override, since
/// picking one of those directly always means Focalors' own built-in
/// look, never Omarchy's.
pub(crate) fn resolve_system_theme(ctx: &egui::Context, theme: UiTheme) -> UiTheme {
    if theme != UiTheme::System {
        *ACTIVE_OMARCHY_PALETTE.lock().unwrap() = None;
        return theme;
    }

    if let Some(palette) = omarchy::read_palette() {
        let resolved = if palette.is_light { UiTheme::Light } else { UiTheme::Dark };
        *ACTIVE_OMARCHY_PALETTE.lock().unwrap() = Some(palette);
        return resolved;
    }

    *ACTIVE_OMARCHY_PALETTE.lock().unwrap() = None;
    match ctx.system_theme() {
        Some(egui::Theme::Dark) => UiTheme::Dark,
        Some(egui::Theme::Light) | None => UiTheme::Light,
    }
}

pub(crate) fn configure_theme(ctx: &egui::Context, theme: UiTheme) {
    let theme = resolve_system_theme(ctx, theme);
    ACTIVE_THEME.store(theme.index(), Ordering::Relaxed);

    let mut visuals = match theme {
        UiTheme::Dark => egui::Visuals::dark(),
        UiTheme::Light => egui::Visuals::light(),
        UiTheme::System => unreachable!("resolve_system_theme never returns System"),
    };
    visuals.override_text_color = Some(hydra_text());
    visuals.panel_fill = hydra_bg();
    visuals.window_fill = hydra_panel_fill();
    visuals.faint_bg_color = hydra_panel_fill();
    visuals.extreme_bg_color = hydra_bg();
    visuals.code_bg_color = hydra_panel_alt_fill();
    visuals.hyperlink_color = hydra_accent();
    visuals.selection.bg_fill = hydra_accent();
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, hydra_text_on_accent());
    visuals.window_stroke = egui::Stroke::new(1.0_f32, hydra_border());
    visuals.window_corner_radius = egui::CornerRadius::same(4);
    visuals.menu_corner_radius = egui::CornerRadius::same(2);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: 0,
        spread: 0,
        color: egui::Color32::TRANSPARENT,
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: 0,
        spread: 0,
        color: egui::Color32::TRANSPARENT,
    };
    visuals.widgets.noninteractive.bg_fill = hydra_panel_fill();
    visuals.widgets.noninteractive.weak_bg_fill = hydra_panel_fill();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, hydra_border());
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.bg_fill = hydra_panel_alt_fill();
    visuals.widgets.inactive.weak_bg_fill = hydra_panel_alt_fill();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, hydra_border());
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.bg_fill = hydra_panel_raised_fill();
    visuals.widgets.hovered.weak_bg_fill = hydra_panel_raised_fill();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, hydra_accent());
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.bg_fill = hydra_panel_raised_fill();
    visuals.widgets.active.weak_bg_fill = hydra_panel_raised_fill();
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, hydra_accent());
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.open.bg_fill = hydra_panel_raised_fill();
    visuals.widgets.open.weak_bg_fill = hydra_panel_raised_fill();
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, hydra_accent());
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(42.0, 30.0);
    style.spacing.menu_margin = egui::Margin::same(12);
    style.spacing.window_margin = egui::Margin::same(16);
    style.visuals.clip_rect_margin = 6.0;
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(26.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(14.5),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(13.5),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(11.5),
    );
    ctx.set_global_style(style);
}

/// Parse the from/to squares of a UCI move ("e2e4", "e7e8q") into square
/// indices for drawing a Game Review arrow. Returns `None` for malformed or
/// empty UCI (UCI is always ASCII, so byte slicing is safe).
/// Drops a puzzle session and gives the shared board back to the start
/// position, unless a game owns it. Returns whether there was a session.

pub(crate) fn format_nodes(nodes: u64) -> String {
    if nodes >= 1_000_000 {
        format!("{:.1}M", nodes as f64 / 1_000_000.0)
    } else if nodes >= 1_000 {
        format!("{:.1}K", nodes as f64 / 1_000.0)
    } else {
        nodes.to_string()
    }
}

pub(crate) fn elapsed_ms(since: Instant) -> u64 {
    since.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn format_clock_ms(ms: u64) -> String {
    let minutes = ms / 60_000;
    let seconds = (ms / 1_000) % 60;
    let tenths = (ms % 1_000) / 100;

    if ms < 60_000 {
        format!("{minutes}:{seconds:02}.{tenths}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The running clock sits on the accent fill of its row, so it reads in
/// plain text; the waiting side recedes. Low time turns red either way.
pub(crate) fn clock_color(remaining_ms: u64, is_active: bool) -> egui::Color32 {
    if remaining_ms < 10_000 {
        hydra_danger()
    } else if is_active {
        hydra_text()
    } else {
        hydra_subtle_text()
    }
}

/// One clock row: name and color on the left, the remaining time on the
/// right. The side to move gets a soft accent fill so the running clock
/// reads at a glance.
pub(crate) fn draw_clock_row(ui: &mut egui::Ui, name: &str, sub: &str, remaining_ms: u64, active: bool) {
    let fill = if active { hydra_clock_active_fill() } else { egui::Color32::TRANSPARENT };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(hydra_heading(name, 13.0));
                    // Grey on the active row's accent fill is too faint in dark
                    // mode; a softened text color reads on both fills.
                    let sub_color = if active {
                        hydra_text().gamma_multiply(0.8)
                    } else {
                        hydra_subtle_text()
                    };
                    ui.label(egui::RichText::new(sub).size(11.0).color(sub_color));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        hydra_heading(format_clock_ms(remaining_ms), 24.0)
                            .color(clock_color(remaining_ms, active)),
                    );
                });
            });
        });
}

pub(crate) fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

pub(crate) fn piece_name(piece: Piece) -> &'static str {
    match piece {
        Piece::Queen => "Queen",
        Piece::Rook => "Rook",
        Piece::Bishop => "Bishop",
        Piece::Knight => "Knight",
        Piece::Pawn => "Pawn",
        Piece::King => "King",
    }
}

pub(crate) fn classification_color(class: crate::analysis::MoveClass) -> egui::Color32 {
    use crate::analysis::MoveClass;
    match class {
        MoveClass::Best | MoveClass::Brilliant => class_best(),
        MoveClass::Good | MoveClass::Forced => hydra_subtle_text(),
        MoveClass::Book => class_book(),
        MoveClass::Inaccuracy => class_inaccuracy(),
        MoveClass::Mistake => class_mistake(),
        MoveClass::Blunder => class_blunder(),
    }
}

pub(crate) fn accuracy_color(accuracy: f64) -> egui::Color32 {
    if accuracy >= 90.0 {
        class_best()
    } else if accuracy >= 70.0 {
        class_inaccuracy()
    } else if accuracy >= 50.0 {
        class_mistake()
    } else {
        class_blunder()
    }
}

