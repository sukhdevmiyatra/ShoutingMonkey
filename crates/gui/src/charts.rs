//! Small painted charts for the overview panel. Thin marks with 4 px
//! rounded data ends, 2 px gaps, recessive grid lines, text in text colours,
//! and a tooltip on every mark.

use egui::{Color32, CornerRadius, FontId, Rect, Sense, Stroke, pos2, vec2};

use crate::theme::Palette;

pub struct Bar<'a> {
    pub label: &'a str,
    pub value: u64,
    /// Overrides the series colour (status colours, for example).
    pub color: Option<Color32>,
}

/// Horizontal bars, one per row: label, bar, count and share of `total`.
/// Returns the index of a clicked bar.
pub fn bar_list(ui: &mut egui::Ui, p: &Palette, bars: &[Bar<'_>], total: u64) -> Option<usize> {
    let max = bars.iter().map(|b| b.value).max().unwrap_or(0).max(1);
    let row_h = 20.0;
    let width = ui.available_width();
    let label_w = (width * 0.42).clamp(90.0, 220.0);
    let value_w = 92.0;
    let track_w = (width - label_w - value_w - 12.0).max(20.0);
    let mut clicked = None;
    for (i, b) in bars.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
        let resp = resp.on_hover_text(format!("{}\n{} URLs ({})", b.label, crate::app::fmt_count(b.value), crate::query::percent(b.value, total)));
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter_at(rect);
        if resp.hovered() {
            painter.rect_filled(rect, 3.0, p.hover);
        }
        let font = FontId::proportional(13.0);
        let label_rect = Rect::from_min_size(rect.min + vec2(4.0, 0.0), vec2(label_w - 8.0, row_h));
        let galley = painter.layout_no_wrap(b.label.to_string(), font.clone(), p.text);
        let clip = painter.with_clip_rect(label_rect);
        clip.galley(pos2(label_rect.left(), rect.center().y - galley.size().y / 2.0), galley, p.text);

        let track = Rect::from_min_size(pos2(rect.left() + label_w, rect.top() + 5.0), vec2(track_w, row_h - 10.0));
        let w = if b.value == 0 { 0.0 } else { (track_w * b.value as f32 / max as f32).max(2.0) };
        if w > 0.0 {
            let bar = Rect::from_min_size(track.min, vec2(w, track.height()));
            painter.rect_filled(bar, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, b.color.unwrap_or(p.series));
        }
        let value = format!("{}  {}", crate::app::fmt_count(b.value), crate::query::percent(b.value, total));
        painter.text(pos2(rect.right() - 4.0, rect.center().y), egui::Align2::RIGHT_CENTER, value, font, p.muted);
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Vertical columns with labels underneath and three faint grid lines.
/// Only the tallest column carries a value label; hover shows the rest.
pub fn columns(ui: &mut egui::Ui, p: &Palette, bars: &[Bar<'_>], height: f32, unit: &str) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if bars.is_empty() || !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let small = FontId::proportional(12.0);
    let axis_w = 40.0;
    let label_h = 18.0;
    let top_pad = 16.0;
    let plot = Rect::from_min_max(pos2(rect.left() + axis_w, rect.top() + top_pad), pos2(rect.right() - 4.0, rect.bottom() - label_h));
    let max = bars.iter().map(|b| b.value).max().unwrap_or(0).max(1);
    let nice = nice_ceiling(max);

    let ticks = tick_count(nice);
    for k in 0..=ticks {
        let v = nice * k / ticks;
        let y = plot.bottom() - plot.height() * (v as f32 / nice as f32);
        let stroke = if k == 0 { Stroke::new(1.0, p.muted) } else { Stroke::new(1.0, p.border) };
        painter.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], stroke);
        painter.text(pos2(plot.left() - 6.0, y), egui::Align2::RIGHT_CENTER, crate::app::fmt_count(v), small.clone(), p.muted);
    }

    let slot = plot.width() / bars.len() as f32;
    let bar_w = (slot - 2.0).clamp(2.0, 36.0);
    let tallest = bars.iter().enumerate().max_by_key(|(_, b)| b.value).map(|(i, _)| i);
    for (i, b) in bars.iter().enumerate() {
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let h = plot.height() * (b.value as f32 / nice as f32);
        let bar = Rect::from_min_max(pos2(cx - bar_w / 2.0, plot.bottom() - h), pos2(cx + bar_w / 2.0, plot.bottom()));
        if b.value > 0 {
            painter.rect_filled(bar, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, b.color.unwrap_or(p.series));
        }
        painter.text(pos2(cx, plot.bottom() + 3.0), egui::Align2::CENTER_TOP, b.label, small.clone(), p.muted);
        if Some(i) == tallest && b.value > 0 {
            painter.text(pos2(cx, bar.top() - 2.0), egui::Align2::CENTER_BOTTOM, crate::app::fmt_count(b.value), small.clone(), p.text);
        }
        // Hit target: the whole column slot, not just the bar.
        let hit = Rect::from_min_max(pos2(cx - slot / 2.0, plot.top()), pos2(cx + slot / 2.0, rect.bottom()));
        let id = ui.scope_id().with(("column", i));
        let resp = ui.interact(hit, id, Sense::hover());
        if resp.hovered() {
            painter.rect_filled(hit, 2.0, p.hover.gamma_multiply(0.5));
            resp.on_hover_text(format!("{} {unit}: {} URLs", b.label, crate::app::fmt_count(b.value)));
        }
    }
}

/// Grid intervals that divide `nice` into whole, round steps.
fn tick_count(nice: u64) -> u64 {
    [4, 5, 2].into_iter().find(|t| nice.is_multiple_of(*t) && nice / t >= 1).unwrap_or(1)
}

/// Round `v` up to 1, 2 or 5 times a power of ten, so grid labels are tidy.
fn nice_ceiling(v: u64) -> u64 {
    let mut magnitude = 1u64;
    while magnitude * 10 <= v {
        magnitude *= 10;
    }
    for step in [1, 2, 5, 10] {
        if step * magnitude >= v {
            return (step * magnitude).max(3);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_numbers() {
        assert_eq!(nice_ceiling(1), 3);
        assert_eq!(nice_ceiling(7), 10);
        assert_eq!(nice_ceiling(42), 50);
        assert_eq!(nice_ceiling(150), 200);
        assert_eq!(nice_ceiling(1000), 1000);
        assert_eq!(tick_count(100), 4);
        assert_eq!(tick_count(5), 5);
        assert_eq!(tick_count(3), 1);
    }
}
