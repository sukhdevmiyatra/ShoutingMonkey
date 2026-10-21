//! Look and feel. The app should read as a native Windows tool: Segoe UI,
//! Windows 11 neutrals, the system accent blue for focus and selection, and
//! colour reserved for meaning (HTTP status classes and the issue bars).

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme, Visuals};

/// Colour tokens. Everything the UI paints comes from here.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Menu bar background and its text and hover.
    pub header: Color32,
    pub on_header: Color32,
    pub header_hover: Color32,
    pub toolbar: Color32,
    pub status: Color32,
    pub window: Color32,
    pub surface: Color32,
    pub zebra: Color32,
    /// Column headings of tables.
    pub table_head: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    /// Group headings such as the Navigator's sections.
    pub section: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub selection: Color32,
    pub hover: Color32,
    /// Chart series colour (validated categorical slot 1).
    pub series: Color32,
    pub ok: Color32,
    pub redirect: Color32,
    pub client_error: Color32,
    pub server_error: Color32,
    pub no_response: Color32,
    pub danger: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// The chrome is plain Windows 11 neutrals with no tint, so the only colour
// on screen carries meaning: HTTP status classes, issue severities, and one
// fixed accent (the Windows default blue) for focus, selection and Start.
// Areas are told apart the way Office and Explorer do it: the menu bar and
// toolbar form a grey band above the white work area, and tables have
// tinted header rows.
pub const LIGHT: Palette = Palette {
    dark: false,
    header: hex(0xE4E4E4),
    on_header: hex(0x1A1A1A),
    header_hover: hex(0xD2D2D2),
    toolbar: hex(0xEBEBEB),
    status: hex(0xE4E4E4),
    window: hex(0xF3F3F3),
    surface: hex(0xFFFFFF),
    zebra: hex(0xF8F8F8),
    table_head: hex(0xEFEFEF),
    border: hex(0xD1D1D1),
    text: hex(0x1A1A1A),
    muted: hex(0x5C5C5C),
    section: hex(0x3B3B3B),
    accent: hex(0x005FB8),
    on_accent: hex(0xFFFFFF),
    selection: hex(0xCCE4F7),
    hover: hex(0xEBEBEB),
    series: hex(0x2A78D6),
    ok: hex(0x0F7B0F),
    redirect: hex(0x9D5D00),
    client_error: hex(0xC42B1C),
    server_error: hex(0x8E2B8E),
    no_response: hex(0x5C5C5C),
    danger: hex(0xC42B1C),
};

// Dark: four distinct neutral steps (status bar, window, tables, headings)
// instead of two near-identical greys, near-white text, and secondary
// text kept above 9:1 on the tables.
pub const DARK: Palette = Palette {
    dark: true,
    header: hex(0x303030),
    on_header: hex(0xF3F3F3),
    header_hover: hex(0x454545),
    toolbar: hex(0x2A2A2A),
    status: hex(0x303030),
    window: hex(0x1F1F1F),
    surface: hex(0x262626),
    zebra: hex(0x2D2D2D),
    table_head: hex(0x363636),
    border: hex(0x4A4A4A),
    text: hex(0xF3F3F3),
    muted: hex(0xC8C8C8),
    section: hex(0xDDDDDD),
    accent: hex(0x60CDFF),
    on_accent: hex(0x002B42),
    selection: hex(0x1E4A6B),
    hover: hex(0x3A3A3A),
    series: hex(0x3987E5),
    ok: hex(0x6CCB5F),
    redirect: hex(0xFCB040),
    client_error: hex(0xFF99A4),
    server_error: hex(0xD6A6F0),
    no_response: hex(0xC8C8C8),
    danger: hex(0xFF99A4),
};

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode { &DARK } else { &LIGHT }
}

/// Colour for an HTTP status (or a missing response).
pub fn status_color(p: &Palette, status: Option<i64>) -> Color32 {
    match status {
        Some(200..=299) => p.ok,
        Some(300..=399) => p.redirect,
        Some(400..=499) => p.client_error,
        Some(500..) => p.server_error,
        _ => p.no_response,
    }
}

/// Colour of an issue type marker (always shown with its label).
pub fn issue_color(p: &Palette, kind: crate::model::Severity) -> Color32 {
    match kind {
        crate::model::Severity::Error => p.client_error,
        crate::model::Severity::Warning => p.redirect,
        crate::model::Severity::Notice => p.ok,
    }
}

/// Apply the saved theme choice and interface size.
pub fn apply_prefs(ctx: &egui::Context, prefs: &crate::prefs::Prefs) {
    use crate::prefs::ThemeChoice;
    ctx.options_mut(|o| {
        o.theme_preference = match prefs.theme {
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::System => egui::ThemePreference::System,
        }
    });
    ctx.set_zoom_factor(prefs.zoom.clamp(0.75, 2.0));
}

/// The semibold family, for headings and the current selection.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, Visuals::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, Visuals::dark()));
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(18.0, semibold())),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 5.0);
        s.button_padding = egui::vec2(12.0, 4.0);
        s.interact_size.y = 28.0;
        s.window_margin = egui::Margin::same(14);
        s.menu_margin = egui::Margin::same(4);
        s.scroll.bar_width = 10.0;
        style.interaction.tooltip_delay = 0.4;
        // Labels behave like a desktop app's: the pointer stays an arrow and a
        // click selects the row under it. Values meant for copying opt back in.
        style.interaction.selectable_labels = false;
        // Motion only where it answers an action.
        style.animation_time = 0.08;
    });
    // The app's own zoom shortcuts save the size; egui's would double them.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

fn visuals(p: &Palette, mut v: Visuals) -> Visuals {
    let radius = CornerRadius::same(3);
    v.panel_fill = p.window;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.zebra;
    v.code_bg_color = p.zebra;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.window_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) };
    v.popup_shadow = egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(30) };
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.hyperlink_color = p.accent;
    v.override_text_color = None;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.window;
    w.noninteractive.weak_bg_fill = p.window;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.surface;
    w.inactive.weak_bg_fill = p.surface;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.selection;
    w.active.weak_bg_fill = p.selection;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    // Open menus and the focused window's title bar stay neutral.
    w.open = w.hovered;
    v
}

/// Segoe UI from the Windows fonts folder, with egui's bundled fonts kept
/// as fallbacks for any glyph Segoe lacks.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("Fonts");
    let load = |file: &str| std::fs::read(dir.join(file)).ok();

    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut proportional = vec![];
    if let Some(bytes) = load("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        proportional.push("segoe".to_string());
    }
    if let Some(bytes) = load("seguisym.ttf") {
        fonts.font_data.insert("segoe-symbol".into(), Arc::new(FontData::from_owned(bytes)));
    }
    let mut semibold_fonts = vec![];
    if let Some(bytes) = load("seguisb.ttf") {
        fonts.font_data.insert("segoe-semibold".into(), Arc::new(FontData::from_owned(bytes)));
        semibold_fonts.push("segoe-semibold".to_string());
    }
    let fallbacks: Vec<String> = fonts
        .font_data
        .contains_key("segoe-symbol")
        .then(|| "segoe-symbol".to_string())
        .into_iter()
        .chain(defaults)
        .collect();
    proportional.extend(fallbacks.iter().cloned());
    semibold_fonts.extend(proportional.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(semibold(), semibold_fonts);
    if let Some(bytes) = load("consola.ttf") {
        fonts.font_data.insert("consolas".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

/// The app icon as an image, for the About box.
pub fn icon_image() -> egui::ColorImage {
    let icon = window_icon();
    egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba)
}

/// The window and taskbar icon: the same logo as the .exe icon.
pub fn window_icon() -> egui::IconData {
    const SIZE: u32 = 128;
    egui::IconData { rgba: crate::logo::render(SIZE), width: SIZE, height: SIZE }
}

/// Text and hover colours for widgets on the menu bar's dark background.
/// Menus that open from it are popups and keep the normal colours.
pub fn header_visuals(ui: &mut egui::Ui, p: &Palette) {
    let w = &mut ui.visuals_mut().widgets;
    for state in [&mut w.noninteractive, &mut w.inactive] {
        state.fg_stroke = Stroke::new(1.0, p.on_header);
        state.weak_bg_fill = Color32::TRANSPARENT;
        state.bg_stroke = Stroke::NONE;
    }
    for state in [&mut w.hovered, &mut w.active, &mut w.open] {
        state.fg_stroke = Stroke::new(1.0, p.on_header);
        state.weak_bg_fill = p.header_hover;
        state.bg_fill = p.header_hover;
        state.bg_stroke = Stroke::NONE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_accents_stay_readable() {
        for p in [LIGHT, DARK] {
            assert!(contrast(p.text, p.surface) >= 12.0, "body text");
            assert!(contrast(p.muted, p.surface) >= 6.0, "secondary text");
            assert!(contrast(p.muted, p.window) >= 5.5, "secondary text on panels");
            assert!(contrast(p.accent, p.surface) >= 4.5, "links and accents");
            assert!(contrast(p.on_accent, p.accent) >= 4.5, "Start button text");
            assert!(contrast(p.on_header, p.header) >= 10.0, "menu bar text");
        }
    }

    fn luminance(c: Color32) -> f32 {
        let ch = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    /// WCAG contrast ratio between two colours.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
}
