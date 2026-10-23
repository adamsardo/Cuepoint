//! Look of the app: palette, fonts and the shared sizes, measured from
//! rekordbox 7.2.19's Export screen on macOS (2x captures, in points).
//!
//! rekordbox draws text in Arial through JUCE. Arial ships with macOS, so it
//! is loaded from the system at startup (nothing is bundled); the egui
//! default fonts remain as the fallback.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};

/// Panel and surface colours.
pub const TITLE_BAR: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const TITLE_TEXT: Color32 = Color32::from_rgb(0xb4, 0xb4, 0xb4);
pub const BLACK: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const BAR: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x0f);
pub const PANEL: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const BUTTON: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
pub const BUTTON_DARK: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
pub const PHRASE_BUTTON: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const STAR_OFF: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x8c);
pub const SEPARATOR: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
pub const TAB_OFF: Color32 = Color32::from_rgb(0x32, 0x32, 0x32);
pub const ROW_EVEN: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x0f);
pub const ROW_ODD: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const ROW_SELECTED: Color32 = Color32::from_rgb(0x31, 0x4b, 0x67);
pub const BORDER: Color32 = Color32::from_rgb(0x53, 0x53, 0x53);
pub const OUTLINE: Color32 = Color32::from_rgb(0x6f, 0x6f, 0x6f);

/// Text colours.
pub const TEXT: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x96, 0x96, 0x96);
pub const TEXT_KEY: Color32 = Color32::from_rgb(0x87, 0x87, 0x87);
pub const TEXT_OFF: Color32 = Color32::from_rgb(0x64, 0x64, 0x64);
pub const TEXT_HINT: Color32 = Color32::from_rgb(0x4b, 0x4b, 0x4b);

/// Accents.
pub const ACCENT: Color32 = Color32::from_rgb(0x35, 0x71, 0xe3);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x4d, 0x86, 0xe7);
pub const BARS_TEXT: Color32 = Color32::from_rgb(0x3d, 0x7c, 0xe0);
pub const HOT_CUE: Color32 = Color32::from_rgb(0xeb, 0x4b, 0x71);
pub const CUE_LETTER: Color32 = Color32::from_rgb(0x2b, 0x0b, 0x14);
pub const CUE_ORANGE: Color32 = Color32::from_rgb(0xf0, 0xa8, 0x4c);
pub const CUE_RING: Color32 = Color32::from_rgb(0xf0, 0xb4, 0x48);
pub const PLAY_RING: Color32 = Color32::from_rgb(0x2e, 0x6b, 0x2b);
pub const PLAY_GLYPH: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
pub const BADGE_BG: Color32 = Color32::from_rgb(0x52, 0x47, 0x12);
pub const BADGE_TEXT: Color32 = Color32::from_rgb(0xf9, 0xda, 0x4b);
pub const STATUS_RED: Color32 = Color32::from_rgb(0xc8, 0x28, 0x1e);
pub const STATUS_GREEN: Color32 = Color32::from_rgb(0x5a, 0xb4, 0x2e);
pub const STATUS_BLUE: Color32 = Color32::from_rgb(0x36, 0x82, 0xe0);
pub const STATUS_CYAN: Color32 = Color32::from_rgb(0x5a, 0xaa, 0xc8);
pub const STATUS_GREY: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x8c);
pub const BAR_LINE: Color32 = Color32::from_rgb(0xe3, 0x2b, 0x2b);
pub const BEAT_LINE: Color32 = Color32::from_rgb(0x4c, 0x4c, 0x4c);

/// 3-band waveform colours. The detail view colours each pixel by which
/// bands reach it; the overview stacks low, mid and high.
pub mod wave {
    use egui::Color32;
    pub const LOW: Color32 = Color32::from_rgb(0x1f, 0x4d, 0xd8);
    pub const MID: Color32 = Color32::from_rgb(0xf2, 0xa6, 0x3b);
    pub const HIGH: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
    pub const LOW_MID: Color32 = Color32::from_rgb(0xa6, 0x66, 0x22);
    pub const MID_HIGH: Color32 = Color32::from_rgb(0xf2, 0xea, 0xd7);
    pub const LOW_HIGH: Color32 = Color32::from_rgb(0xd2, 0xdb, 0xf6);
    pub const ALL: Color32 = Color32::from_rgb(0xfc, 0xf1, 0xd8);
    pub const OVERVIEW_LOW: Color32 = Color32::from_rgb(0x22, 0x54, 0xd9);
    pub const OVERVIEW_MID: Color32 = Color32::from_rgb(0xf2, 0xaa, 0x3c);
    pub const RAIL_PLAYED: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
    pub const RAIL: Color32 = Color32::from_rgb(0x6b, 0x6b, 0x6b);
    pub const RAIL_DARK: Color32 = Color32::from_rgb(0x51, 0x51, 0x51);
}

/// A hot cue's display colour: rekordbox's pink for the default red-pink
/// stored by its XML export, otherwise the stored colour.
pub fn cue_colour(cue: &cuepoint_core::Cue) -> Color32 {
    match cue.colour {
        None | Some(0xFF376F) => HOT_CUE,
        Some(rgb) => Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8),
    }
}

/// Font families registered by [`install`].
pub const BOLD: &str = "bold";

/// JUCE sizes fonts by ascent + descent; Arial's is 1.117 em, so JUCE
/// height 12 is this em size, which is what egui uses.
pub const BODY_SIZE: f32 = 12.0 / 1.117;

pub fn body() -> FontId {
    FontId::proportional(BODY_SIZE)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

const ARIAL: &str = "/System/Library/Fonts/Supplemental/Arial.ttf";
const ARIAL_BOLD: &str = "/System/Library/Fonts/Supplemental/Arial Bold.ttf";

/// Registers Arial (regular and bold) ahead of egui's defaults and applies
/// the dark palette.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, path: &str, family: FontFamily| {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.into());
        }
    };
    add("Arial", ARIAL, FontFamily::Proportional);
    add("Arial Bold", ARIAL_BOLD, FontFamily::Name(BOLD.into()));
    // The bold family falls back to the proportional fonts for missing glyphs.
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts
        .families
        .entry(FontFamily::Name(BOLD.into()))
        .or_default()
        .extend(fallback);
    ctx.set_fonts(fonts);

    ctx.global_style_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BLACK;
        style.visuals.selection.bg_fill = ROW_SELECTED;
        style.visuals.override_text_color = Some(TEXT);
        style.spacing.item_spacing = egui::vec2(4.0, 2.0);
        for font in style.text_styles.values_mut() {
            font.size = BODY_SIZE;
        }
    });
}
