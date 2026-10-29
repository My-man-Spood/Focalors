//! Reads Omarchy's active theme palette, for the "Match System" UI theme
//! option. Linux/Omarchy-specific; everywhere else "Match System" just
//! follows the OS's plain light/dark signal (see `resolve_system_theme`
//! in `mod.rs`).
//!
//! Omarchy writes its active theme to `~/.local/state/omarchy/current/`:
//! `theme.name` (plain text) and `theme/colors.toml` (the palette, in
//! Omarchy's own "Quattro" format — named keys like `background`,
//! `foreground`, `accent`, with optional `mode = "light"/"dark"`). This
//! only reads those two files; it never writes or watches them — callers
//! just re-check periodically, which is cheap enough not to need a real
//! filesystem watcher for two small files.

use super::*;

/// The handful of UI chrome colors "Match System" can override on Omarchy.
/// Deliberately narrow: semantic colors (success/warning/danger, and the
/// Game Review move-classification colors) are left alone, since those
/// carry meaning a theme shouldn't be free to recolor away.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OmarchyPalette {
    pub(crate) bg: egui::Color32,
    pub(crate) panel: egui::Color32,
    pub(crate) panel_alt: egui::Color32,
    pub(crate) panel_raised: egui::Color32,
    pub(crate) text: egui::Color32,
    pub(crate) subtle_text: egui::Color32,
    pub(crate) border: egui::Color32,
    pub(crate) accent: egui::Color32,
    pub(crate) accent_soft: egui::Color32,
    pub(crate) text_on_accent: egui::Color32,
    pub(crate) is_light: bool,
}

fn state_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local/state/omarchy/current")
}

/// The active theme's display name (from `theme.name`), if Omarchy is
/// present and its state is currently readable.
pub(crate) fn active_theme_name() -> Option<String> {
    std::fs::read_to_string(state_dir().join("theme.name"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Reads and parses Omarchy's current palette. `None` if Omarchy isn't
/// installed, hasn't picked a theme, or the theme is mid-switch and
/// momentarily unreadable — callers already poll periodically, so a
/// transient miss just resolves again next time.
pub(crate) fn read_palette() -> Option<OmarchyPalette> {
    let colors_path = state_dir().join("theme/colors.toml");
    let source = std::fs::read_to_string(colors_path).ok()?;
    let values: toml::Value = toml::from_str(&source).ok()?;

    let get = |key: &str| -> Option<[u8; 3]> {
        values.get(key).and_then(toml::Value::as_str).and_then(parse_hex_rgb)
    };

    let bg = get("background")?;
    let text = get("foreground")?;
    let accent = get("accent")?;
    let panel = get("dark_background").unwrap_or(bg);
    let border = get("muted").unwrap_or_else(|| blend(bg, text, 0.36));

    let is_light = match values.get("mode").and_then(toml::Value::as_str) {
        Some("light") => true,
        Some("dark") => false,
        _ => luminance(bg) > 0.45,
    };

    let panel_alt = blend(panel, bg, 0.5);
    let panel_raised = blend(panel, text, 0.12);
    let subtle_text = blend(text, bg, 0.42);
    let accent_soft_alpha = if is_light { 24 } else { 34 };
    let text_on_accent = if luminance(accent) > 0.45 { [20, 20, 20] } else { [246, 248, 252] };

    Some(OmarchyPalette {
        bg: to_color32(bg),
        panel: to_color32(panel),
        panel_alt: to_color32(panel_alt),
        panel_raised: to_color32(panel_raised),
        text: to_color32(text),
        subtle_text: to_color32(subtle_text),
        border: to_color32(border),
        accent: to_color32(accent),
        accent_soft: egui::Color32::from_rgba_unmultiplied(
            accent[0], accent[1], accent[2], accent_soft_alpha,
        ),
        text_on_accent: to_color32(text_on_accent),
        is_light,
    })
}

fn to_color32(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Accepts "#rrggbb" or "rrggbb"; anything else (an unexpected format in a
/// theme Omarchy's own tooling produced) is treated as missing, same as a
/// key that isn't there at all.
fn parse_hex_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&s[range], 16).ok();
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

fn blend(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t) as u8)
}

/// Relative (perceptual) luminance, same formula used for WCAG contrast —
/// just needed here to decide "is this background light or dark" when the
/// theme doesn't say so explicitly via `mode`.
fn luminance(rgb: [u8; 3]) -> f64 {
    let channel = |c: u8| {
        let v = f64::from(c) / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    channel(rgb[0]) * 0.2126 + channel(rgb[1]) * 0.7152 + channel(rgb[2]) * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_hash() {
        assert_eq!(parse_hex_rgb("#1a1b26"), Some([0x1a, 0x1b, 0x26]));
        assert_eq!(parse_hex_rgb("1a1b26"), Some([0x1a, 0x1b, 0x26]));
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(parse_hex_rgb("#1a1b2"), None);
        assert_eq!(parse_hex_rgb("not-a-color"), None);
        assert_eq!(parse_hex_rgb(""), None);
    }

    #[test]
    fn luminance_orders_black_below_white() {
        assert!(luminance([0, 0, 0]) < luminance([255, 255, 255]));
    }

    #[test]
    fn blend_at_zero_and_one_returns_the_endpoints() {
        let a = [10, 20, 30];
        let b = [200, 150, 100];
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
    }

    /// Reads the *real* `~/.local/state/omarchy/current/` on whatever
    /// machine runs this — not a fixture. Ignored by default since most
    /// machines (and CI) aren't running Omarchy at all; run with
    /// `cargo test -- --ignored` on an actual Omarchy box to sanity-check
    /// the parser against a real theme file instead of a hand-written one.
    #[test]
    #[ignore]
    fn real_omarchy_state_parses_if_present() {
        let Some(name) = active_theme_name() else {
            eprintln!("No Omarchy state found at {:?} — nothing to check here.", state_dir());
            return;
        };
        let palette = read_palette().expect("theme.name exists but colors.toml didn't parse");
        eprintln!("Omarchy theme: {name:?}, is_light: {}", palette.is_light);
        eprintln!(
            "bg={:?} panel={:?} text={:?} accent={:?} border={:?}",
            palette.bg, palette.panel, palette.text, palette.accent, palette.border
        );
    }
}
