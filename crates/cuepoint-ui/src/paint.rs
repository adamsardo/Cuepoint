//! Painting helpers for the custom-drawn screens: text placed by cap
//! height, flat buttons, and a small set of original vector icons.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{
    Align, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};

use crate::theme;

/// Arial's ascent minus its cap height, as a fraction of the em size: the
/// gap between a galley's top and the top of its capital letters.
const CAP_OFFSET: f32 = 0.905 - 0.716;
/// egui places Arial's caps about 0.4 pt lower than JUCE does.
const CAP_NUDGE: f32 = 0.4;

/// Draws `text` with its capital letters starting at `cap_top`, anchored
/// horizontally at `x` by `align`. Returns the galley's rect.
pub fn text(
    painter: &Painter,
    x: f32,
    cap_top: f32,
    align: Align,
    text: &str,
    font: FontId,
    colour: Color32,
) -> Rect {
    let size = font.size;
    let galley = painter.layout_no_wrap(text.to_owned(), font, colour);
    let width = galley.size().x;
    let left = match align {
        Align::Min => x,
        Align::Center => x - width / 2.0,
        Align::Max => x - width,
    };
    let top = cap_top - CAP_OFFSET * size - CAP_NUDGE;
    let rect = Rect::from_min_size(pos2(left, top), galley.size());
    painter.galley(rect.min, galley, colour);
    rect
}

/// Like [`text`], with `spacing` points added between letters.
#[allow(clippy::too_many_arguments)]
pub fn text_spaced(
    painter: &Painter,
    x: f32,
    cap_top: f32,
    align: Align,
    label: &str,
    font: FontId,
    colour: Color32,
    spacing: f32,
) -> Rect {
    let size = font.size;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        label,
        0.0,
        egui::TextFormat {
            font_id: font,
            color: colour,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let width = galley.size().x;
    let left = match align {
        Align::Min => x,
        Align::Center => x - width / 2.0,
        Align::Max => x - width,
    };
    let rect = Rect::from_min_size(
        pos2(left, cap_top - CAP_OFFSET * size - CAP_NUDGE),
        galley.size(),
    );
    painter.galley(rect.min, galley, colour);
    rect
}

/// Like [`text`], but clipped to `max_width` with nothing drawn past it.
#[allow(clippy::too_many_arguments)]
pub fn text_clipped(
    painter: &Painter,
    x: f32,
    cap_top: f32,
    max_width: f32,
    align: Align,
    label: &str,
    font: FontId,
    colour: Color32,
) {
    let clip = painter.clip_rect().intersect(Rect::from_x_y_ranges(
        match align {
            Align::Max => x - max_width..=x,
            Align::Center => x - max_width / 2.0..=x + max_width / 2.0,
            Align::Min => x..=x + max_width,
        },
        cap_top - 20.0..=cap_top + 40.0,
    ));
    text(
        &painter.with_clip_rect(clip),
        x,
        cap_top,
        align,
        label,
        font,
        colour,
    );
}

/// Text centred in `rect`, by cap height.
pub fn text_centred(painter: &Painter, rect: Rect, label: &str, font: FontId, colour: Color32) {
    let cap = 0.716 * font.size;
    let cap_top = (rect.center().y - cap / 2.0).round();
    text(
        painter,
        rect.center().x,
        cap_top,
        Align::Center,
        label,
        font,
        colour,
    );
}

pub fn fill(painter: &Painter, rect: Rect, colour: Color32) {
    painter.rect_filled(rect, CornerRadius::ZERO, colour);
}

pub fn rounded(painter: &Painter, rect: Rect, radius: f32, colour: Color32) {
    painter.rect_filled(rect, radius, colour);
}

pub fn outline(painter: &Painter, rect: Rect, radius: f32, colour: Color32) {
    painter.rect_stroke(rect, radius, Stroke::new(1.0, colour), StrokeKind::Inside);
}

pub fn line(painter: &Painter, a: Pos2, b: Pos2, width: f32, colour: Color32) {
    painter.line_segment([a, b], Stroke::new(width, colour));
}

pub fn polyline(painter: &Painter, points: Vec<Pos2>, width: f32, colour: Color32) {
    painter.add(PathShape::line(points, PathStroke::new(width, colour)));
}

pub fn triangle(painter: &Painter, points: [Pos2; 3], colour: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), colour, Stroke::NONE));
}

/// A `v` chevron centred at `c`, `w` wide.
pub fn chevron_down(painter: &Painter, c: Pos2, w: f32, colour: Color32) {
    let h = w * 0.5;
    polyline(
        painter,
        vec![
            c + vec2(-w / 2.0, -h / 2.0),
            c + vec2(0.0, h / 2.0),
            c + vec2(w / 2.0, -h / 2.0),
        ],
        1.5,
        colour,
    );
}

pub fn chevron_up(painter: &Painter, c: Pos2, w: f32, colour: Color32) {
    let h = w * 0.5;
    polyline(
        painter,
        vec![
            c + vec2(-w / 2.0, h / 2.0),
            c + vec2(0.0, -h / 2.0),
            c + vec2(w / 2.0, h / 2.0),
        ],
        1.5,
        colour,
    );
}

/// `<` (or `>` when `right`) chevron centred at `c`, `h` tall.
pub fn chevron_side(painter: &Painter, c: Pos2, h: f32, right: bool, colour: Color32) {
    let w = h * 0.5 * if right { 1.0 } else { -1.0 };
    polyline(
        painter,
        vec![
            c + vec2(-w / 2.0, -h / 2.0),
            c + vec2(w / 2.0, 0.0),
            c + vec2(-w / 2.0, h / 2.0),
        ],
        1.5,
        colour,
    );
}

/// Solid triangle pointing left or right, centred at `c`.
pub fn play_triangle(painter: &Painter, c: Pos2, h: f32, right: bool, colour: Color32) {
    let w = h * 0.85 * if right { 1.0 } else { -1.0 };
    triangle(
        painter,
        [
            c + vec2(-w / 2.0, -h / 2.0),
            c + vec2(w / 2.0, 0.0),
            c + vec2(-w / 2.0, h / 2.0),
        ],
        colour,
    );
}

/// Solid triangle pointing down (expanded tree node) or right (collapsed).
pub fn disclosure(painter: &Painter, c: Pos2, size: f32, open: bool, colour: Color32) {
    let s = size / 2.0;
    let points = if open {
        [
            c + vec2(-s, -s * 0.8),
            c + vec2(s, -s * 0.8),
            c + vec2(0.0, s * 0.9),
        ]
    } else {
        [
            c + vec2(-s * 0.8, -s),
            c + vec2(s * 0.9, 0.0),
            c + vec2(-s * 0.8, s),
        ]
    };
    triangle(painter, points, colour);
}

pub fn cross(painter: &Painter, c: Pos2, size: f32, width: f32, colour: Color32) {
    let s = size / 2.0;
    line(painter, c + vec2(-s, -s), c + vec2(s, s), width, colour);
    line(painter, c + vec2(-s, s), c + vec2(s, -s), width, colour);
}

/// Three stacked bars.
pub fn hamburger(painter: &Painter, c: Pos2, w: f32, colour: Color32) {
    for dy in [-6.0, 0.0, 6.0] {
        fill(
            painter,
            Rect::from_center_size(c + vec2(0.0, dy), vec2(w, 2.5)),
            colour,
        );
    }
}

/// Magnifying glass with an optional `+`/`-` inside.
pub fn magnifier(painter: &Painter, c: Pos2, r: f32, sign: Option<bool>, colour: Color32) {
    painter.circle_stroke(c, r, Stroke::new(1.3, colour));
    let d = r * std::f32::consts::FRAC_1_SQRT_2;
    line(
        painter,
        c + vec2(d, d),
        c + vec2(d + r * 0.6, d + r * 0.6),
        1.6,
        colour,
    );
    if let Some(plus) = sign {
        let s = r * 0.5;
        line(painter, c + vec2(-s, 0.0), c + vec2(s, 0.0), 1.2, colour);
        if plus {
            line(painter, c + vec2(0.0, -s), c + vec2(0.0, s), 1.2, colour);
        }
    }
}

/// A "skip to" arrow with a stop bar: `←|` when `left`, else `|→`.
pub fn jump_arrow(painter: &Painter, c: Pos2, left: bool, colour: Color32) {
    let dir = if left { -1.0 } else { 1.0 };
    let tip = c + vec2(5.0 * dir, 0.0);
    let tail = c + vec2(-5.0 * dir, 0.0);
    line(painter, tail, tip, 1.6, colour);
    line(painter, tip, tip + vec2(-3.0 * dir, -3.0), 1.6, colour);
    line(painter, tip, tip + vec2(-3.0 * dir, 3.0), 1.6, colour);
    let bar = if left {
        c + vec2(6.5, 0.0)
    } else {
        c + vec2(-6.5, 0.0)
    };
    line(
        painter,
        bar + vec2(0.0, -4.0),
        bar + vec2(0.0, 4.0),
        1.6,
        colour,
    );
}

/// Gear: a ring with eight teeth.
pub fn gear(painter: &Painter, c: Pos2, r: f32, colour: Color32) {
    let mut points = Vec::new();
    for i in 0..32 {
        let a = i as f32 / 32.0 * std::f32::consts::TAU;
        let tooth = (i / 2) % 2 == 0;
        let radius = if tooth { r } else { r * 0.78 };
        points.push(c + Vec2::angled(a) * radius);
    }
    painter.add(Shape::convex_polygon(points, colour, Stroke::NONE));
    painter.circle_filled(c, r * 0.38, theme::BAR);
}

/// A circle outline with a filled centre, used for the recording button.
pub fn record(painter: &Painter, c: Pos2, colour: Color32) {
    painter.circle_stroke(c, 6.2, Stroke::new(1.5, colour));
    painter.circle_filled(c, 3.8, colour);
}

/// A rounded square badge with an `i`.
pub fn info(painter: &Painter, c: Pos2, colour: Color32, background: Color32) {
    let mut points = Vec::new();
    for i in 0..6 {
        let a = (i as f32 / 6.0 + 1.0 / 12.0) * std::f32::consts::TAU;
        points.push(c + Vec2::angled(a) * 6.5);
    }
    painter.add(Shape::convex_polygon(points, colour, Stroke::NONE));
    painter.circle_filled(c + vec2(0.0, -2.6), 1.1, background);
    fill(
        painter,
        Rect::from_center_size(c + vec2(0.0, 1.4), vec2(2.0, 5.0)),
        background,
    );
}

/// An open arc, like a busy/sync indicator.
pub fn arc(painter: &Painter, c: Pos2, r: f32, from: f32, to: f32, width: f32, colour: Color32) {
    let steps = 24;
    let points = (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            c + Vec2::angled(a) * r
        })
        .collect();
    polyline(painter, points, width, colour);
}

/// A music note.
pub fn note(painter: &Painter, c: Pos2, colour: Color32) {
    painter.circle_filled(c + vec2(-2.0, 4.0), 2.6, colour);
    line(
        painter,
        c + vec2(0.4, 4.0),
        c + vec2(0.4, -6.0),
        1.2,
        colour,
    );
    painter.add(CubicBezierShape::from_points_stroke(
        [
            c + vec2(0.4, -6.0),
            c + vec2(3.5, -5.0),
            c + vec2(4.5, -2.0),
            c + vec2(3.0, 0.5),
        ],
        false,
        Color32::TRANSPARENT,
        Stroke::new(1.2, colour),
    ));
}

/// A folder: tab plus body.
pub fn folder(painter: &Painter, rect: Rect, colour: Color32) {
    let tab = Rect::from_min_size(rect.min, vec2(rect.width() * 0.42, 3.0));
    rounded(painter, tab, 1.0, colour);
    let body = Rect::from_min_max(rect.min + vec2(0.0, 2.0), rect.max);
    rounded(painter, body, 1.0, colour);
}

/// A playlist: a page of lines with bullets.
pub fn playlist(painter: &Painter, rect: Rect, colour: Color32) {
    outline(painter, rect, 0.0, colour);
    for i in 0..3 {
        let y = rect.top() + 2.5 + i as f32 * 2.7;
        fill(
            painter,
            Rect::from_min_size(pos2(rect.left() + 2.0, y), vec2(1.5, 1.5)),
            colour,
        );
        fill(
            painter,
            Rect::from_min_max(
                pos2(rect.left() + 4.5, y),
                pos2(rect.right() - 2.0, y + 1.5),
            ),
            colour,
        );
    }
}

/// A padlock.
pub fn lock(painter: &Painter, c: Pos2, colour: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, 1.5), vec2(8.0, 6.0));
    rounded(painter, body, 1.0, colour);
    arc(
        painter,
        c + vec2(0.0, -1.5),
        2.6,
        std::f32::consts::PI,
        std::f32::consts::TAU,
        1.4,
        colour,
    );
}

/// An open padlock: body with the shackle lifted to one side.
pub fn open_lock(painter: &Painter, c: Pos2, colour: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, 3.0), vec2(11.0, 8.0));
    rounded(painter, body, 1.5, colour);
    painter.circle_filled(c + vec2(0.0, 2.5), 1.5, theme::BUTTON);
    fill(
        painter,
        Rect::from_center_size(c + vec2(0.0, 4.5), vec2(1.2, 2.5)),
        theme::BUTTON,
    );
    arc(
        painter,
        c + vec2(0.0, -2.5),
        3.8,
        std::f32::consts::PI,
        std::f32::consts::TAU,
        1.8,
        colour,
    );
    line(
        painter,
        c + vec2(-3.8, -2.5),
        c + vec2(-3.8, -1.0),
        1.8,
        colour,
    );
}

/// A curved undo (or redo) arrow.
pub fn curved_arrow(painter: &Painter, c: Pos2, undo: bool, colour: Color32) {
    let dir = if undo { 1.0 } else { -1.0 };
    let centre = c + vec2(dir * 0.5, 1.5);
    let (from, to) = if undo { (3.6, 7.0) } else { (-0.5, -3.9) };
    arc(painter, centre, 4.5, from, to, 1.6, colour);
    let tip = centre + Vec2::angled(from) * 4.5;
    triangle(
        painter,
        [
            tip + vec2(-dir * 3.0, 0.0),
            tip + vec2(dir * 1.5, -3.0),
            tip + vec2(dir * 1.5, 3.0),
        ],
        colour,
    );
}

/// A plus inside a circle.
pub fn plus_circle(painter: &Painter, c: Pos2, colour: Color32) {
    painter.circle_filled(c, 5.0, colour);
    line(
        painter,
        c + vec2(-3.0, 0.0),
        c + vec2(3.0, 0.0),
        1.2,
        theme::PANEL,
    );
    line(
        painter,
        c + vec2(0.0, -3.0),
        c + vec2(0.0, 3.0),
        1.2,
        theme::PANEL,
    );
}

/// A five-pointed star.
pub fn star(painter: &Painter, c: Pos2, r: f32, colour: Color32) {
    let points: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let radius = if i % 2 == 0 { r } else { r * 0.45 };
            c + Vec2::angled(a) * radius
        })
        .collect();
    // Concave, so draw it as a fan of triangles around the centre.
    for i in 0..10 {
        triangle(painter, [c, points[i], points[(i + 1) % 10]], colour);
    }
}

/// A funnel (filter).
pub fn funnel(painter: &Painter, c: Pos2, colour: Color32) {
    triangle(
        painter,
        [
            c + vec2(-5.5, -4.5),
            c + vec2(5.5, -4.5),
            c + vec2(0.0, 1.5),
        ],
        colour,
    );
    fill(
        painter,
        Rect::from_min_max(c + vec2(-1.2, 0.0), c + vec2(1.2, 5.0)),
        colour,
    );
}

/// A sort arrow (up when `ascending`).
pub fn sort_arrow(painter: &Painter, c: Pos2, ascending: bool, colour: Color32) {
    let dir = if ascending { 1.0 } else { -1.0 };
    line(
        painter,
        c + vec2(0.0, -5.5 * dir),
        c + vec2(0.0, 5.5 * dir),
        1.4,
        colour,
    );
    line(
        painter,
        c + vec2(0.0, -5.5 * dir),
        c + vec2(-3.5, -2.0 * dir),
        1.4,
        colour,
    );
    line(
        painter,
        c + vec2(0.0, -5.5 * dir),
        c + vec2(3.5, -2.0 * dir),
        1.4,
        colour,
    );
}

/// A list icon inside a square outline: bullets and lines.
pub fn list_icon(painter: &Painter, rect: Rect, bullets: bool, colour: Color32) {
    for i in 0..4 {
        let y = rect.top() + 3.0 + i as f32 * 2.6;
        let left = if bullets {
            fill(
                painter,
                Rect::from_min_size(pos2(rect.left() + 3.0, y), vec2(1.6, 1.6)),
                colour,
            );
            rect.left() + 5.5
        } else {
            rect.left() + 3.0
        };
        fill(
            painter,
            Rect::from_min_max(pos2(left, y), pos2(rect.right() - 3.0, y + 1.6)),
            colour,
        );
    }
}

/// Vertical bars inside a square outline (column layouts).
pub fn columns_icon(painter: &Painter, rect: Rect, count: usize, colour: Color32) {
    let inner = rect.shrink(3.0);
    let gap = 1.2;
    let w = (inner.width() - gap * (count as f32 - 1.0)) / count as f32;
    for i in 0..count {
        let x = inner.left() + i as f32 * (w + gap);
        fill(
            painter,
            Rect::from_min_max(pos2(x, inner.top()), pos2(x + w, inner.bottom())),
            colour,
        );
    }
}

/// Rounds a coordinate to the device pixel grid at 2x.
pub fn snap(v: f32) -> f32 {
    (v * 2.0).round() / 2.0
}
