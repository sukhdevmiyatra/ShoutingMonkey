//! A spreadsheet-style table for the side and bottom panels: a fixed
//! header, columns resized by dragging their edge (double-click fits the
//! content), sorting by clicking a heading, and scrollbars in both
//! directions. Its size is whatever it is given; wide content scrolls
//! inside it and never pushes the panel around it.

use std::cmp::Ordering;

use egui::{Color32, FontId, Sense, Stroke, vec2};

use crate::theme::{self, Palette};

const ROW_HEIGHT: f32 = 24.0;
const HEADER_HEIGHT: f32 = 27.0;
/// Padding inside a cell, each side.
const PAD: f32 = 6.0;
const MIN_WIDTH: f32 = 32.0;
/// Rows measured when fitting a column to its content.
const FIT_SAMPLE: usize = 2000;

pub struct GridCol<'a> {
    pub title: &'a str,
    pub width: f32,
    pub numeric: bool,
}

impl<'a> GridCol<'a> {
    pub fn text(title: &'a str, width: f32) -> Self {
        Self { title, width, numeric: false }
    }

    pub fn num(title: &'a str, width: f32) -> Self {
        Self { title, width, numeric: true }
    }
}

#[derive(Default, Clone)]
pub struct Cell {
    pub text: String,
    /// Sort key for numeric columns.
    pub num: Option<f64>,
    pub color: Option<Color32>,
    /// A small coloured square before the text (status, severity).
    pub chip: Option<Color32>,
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Self {
        Self { text: s.into(), ..Default::default() }
    }

    pub fn num(n: f64, shown: impl Into<String>) -> Self {
        Self { text: shown.into(), num: Some(n), ..Default::default() }
    }

    pub fn color(mut self, c: Color32) -> Self {
        self.color = Some(c);
        self
    }

    pub fn chip(mut self, c: Color32) -> Self {
        self.chip = Some(c);
        self
    }
}

/// What happened in the grid this frame. Row numbers index the caller's
/// rows, whatever order they are shown in.
#[derive(Default)]
pub struct GridResponse {
    pub clicked: Option<(usize, usize)>,
    pub double_clicked: Option<usize>,
}

#[derive(Clone, Default)]
struct State {
    widths: Vec<f32>,
    sort: Option<(usize, bool)>,
}

/// Show `rows` (one `Cell` per column) filling the space `ui` offers.
pub fn grid(ui: &mut egui::Ui, p: &Palette, id: &str, cols: &[GridCol], rows: &[Vec<Cell>], selected: Option<usize>) -> GridResponse {
    let id = ui.make_persistent_id(("grid", id));
    let mut state: State = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if state.widths.len() != cols.len() {
        state.widths = cols.iter().map(|c| c.width).collect();
        state.sort = None;
    }
    let order = sorted(rows, state.sort);
    let total_w: f32 = state.widths.iter().sum();
    let font = FontId::proportional(13.5);
    let mut out = GridResponse::default();

    // The header is reserved first and painted after the body, so it
    // follows the body's horizontal scroll in the same frame.
    let (header_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
    let prev_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let body = egui::ScrollArea::both().id_salt(id.with("body")).auto_shrink([false, false]).show_rows(ui, ROW_HEIGHT, order.len(), |ui, range| {
        ui.set_min_width(total_w);
        for shown in range {
            let ri = order[shown];
            let (rect, _) = ui.allocate_exact_size(vec2(total_w.max(ui.available_width()), ROW_HEIGHT), Sense::hover());
            let resp = ui.interact(rect, id.with(("row", ri)), Sense::click());
            let fill = if selected == Some(ri) {
                Some(p.selection)
            } else if resp.hovered() {
                Some(p.hover)
            } else if shown % 2 == 1 {
                Some(p.zebra)
            } else {
                None
            };
            if let Some(f) = fill {
                ui.painter().rect_filled(rect, 0.0, f);
            }
            let mut x = rect.left();
            let mut hovered_cut: Option<&str> = None;
            let pointer = resp.hover_pos();
            for (ci, w) in state.widths.iter().enumerate() {
                let cell_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(*w, ROW_HEIGHT));
                x += w;
                if !ui.is_rect_visible(cell_rect) {
                    continue;
                }
                let painter = ui.painter();
                painter.line_segment([cell_rect.right_top(), cell_rect.right_bottom()], Stroke::new(1.0, p.border.gamma_multiply(0.5)));
                let Some(c) = rows[ri].get(ci) else { continue };
                if pointer.is_some_and(|q| cell_rect.contains(q)) && resp.clicked() {
                    out.clicked = Some((ri, ci));
                }
                let inner = cell_rect.shrink2(vec2(PAD, 0.0));
                let mut tx = inner.left();
                if let Some(chip) = c.chip {
                    let r = egui::Rect::from_center_size(egui::pos2(tx + 4.0, inner.center().y), vec2(8.0, 8.0));
                    painter.rect_filled(r, 2.0, chip);
                    tx += 14.0;
                }
                let color = c.color.unwrap_or(p.text);
                let galley = painter.layout_no_wrap(c.text.clone(), font.clone(), color);
                let room = inner.right() - tx;
                if galley.size().x > room && pointer.is_some_and(|q| cell_rect.contains(q)) {
                    hovered_cut = Some(&c.text);
                }
                if cols[ci].numeric {
                    tx = (inner.right() - galley.size().x).max(tx);
                }
                painter
                    .with_clip_rect(inner.intersect(ui.clip_rect()))
                    .galley(egui::pos2(tx, inner.center().y - galley.size().y / 2.0), galley, color);
            }
            if resp.double_clicked() {
                out.double_clicked = Some(ri);
            }
            if let Some(full) = hovered_cut {
                resp.on_hover_text_at_pointer(full);
            }
        }
    });
    ui.spacing_mut().item_spacing.y = prev_spacing;

    header(ui, p, id, header_rect, cols, rows, &mut state, body.state.clamped_offset().x, &font);
    ui.data_mut(|d| d.insert_temp(id, state));
    out
}

#[allow(clippy::too_many_arguments)]
fn header(ui: &mut egui::Ui, p: &Palette, id: egui::Id, rect: egui::Rect, cols: &[GridCol], rows: &[Vec<Cell>], state: &mut State, offset_x: f32, font: &FontId) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, p.table_head);
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, p.border));
    let head_font = FontId::new(13.5, theme::semibold());
    let mut x = rect.left() - offset_x;
    let mut fit: Option<usize> = None;
    let mut fit_all = false;
    let mut reset = false;
    for (ci, col) in cols.iter().enumerate() {
        let w = state.widths[ci];
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, rect.height()));
        x += w;
        if cell.right() < rect.left() || cell.left() > rect.right() {
            continue;
        }
        let hit = cell.intersect(rect);
        let resp = ui.interact(hit, id.with(("head", ci)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColumnHeader, true, col.title));
        if resp.hovered() {
            painter.rect_filled(hit, 0.0, p.hover);
        }
        let arrow = match state.sort {
            Some((c, true)) if c == ci => " ▼",
            Some((c, false)) if c == ci => " ▲",
            _ => "",
        };
        let galley = painter.layout_no_wrap(format!("{}{arrow}", col.title), head_font.clone(), p.text);
        let inner = cell.shrink2(vec2(PAD, 0.0));
        let tx = if col.numeric { (inner.right() - galley.size().x).max(inner.left()) } else { inner.left() };
        painter.with_clip_rect(inner.intersect(rect)).galley(egui::pos2(tx, cell.center().y - galley.size().y / 2.0), galley, p.text);
        painter.line_segment([cell.right_top() + vec2(0.0, 4.0), cell.right_bottom() - vec2(0.0, 4.0)], Stroke::new(1.0, p.border));
        if resp.clicked() {
            // Numbers sort largest first, text A to Z; a second click flips it.
            state.sort = match state.sort {
                Some((c, desc)) if c == ci => Some((ci, !desc)),
                _ => Some((ci, col.numeric)),
            };
        }
        resp.on_hover_text(format!("{}\nClick to sort. Drag the edge to resize; double-click it to fit.", col.title)).context_menu(|ui| {
            if ui.button("Fit Column to Content").clicked() {
                fit = Some(ci);
                ui.close();
            }
            if ui.button("Fit All Columns").clicked() {
                fit_all = true;
                ui.close();
            }
            if ui.button("Reset Column Widths").clicked() {
                reset = true;
                ui.close();
            }
        });
        let handle = egui::Rect::from_center_size(egui::pos2(cell.right(), cell.center().y), vec2(8.0, rect.height()));
        let h = ui.interact(handle.intersect(rect), id.with(("resize", ci)), Sense::click_and_drag());
        if h.hovered() || h.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if h.dragged() {
            state.widths[ci] = (w + h.drag_delta().x).clamp(MIN_WIDTH, 2000.0);
        }
        if h.double_clicked() {
            fit = Some(ci);
        }
    }
    let measure = |ci: usize| -> f32 {
        let title = painter.layout_no_wrap(format!("{} ▼", cols[ci].title), head_font.clone(), p.text).size().x;
        let content = rows
            .iter()
            .take(FIT_SAMPLE)
            .filter_map(|r| r.get(ci))
            .map(|c| painter.layout_no_wrap(c.text.clone(), font.clone(), p.text).size().x + if c.chip.is_some() { 14.0 } else { 0.0 })
            .fold(0.0, f32::max);
        (title.max(content) + 2.0 * PAD + 2.0).clamp(MIN_WIDTH, 900.0)
    };
    if let Some(ci) = fit {
        state.widths[ci] = measure(ci);
    }
    if fit_all {
        for ci in 0..cols.len() {
            state.widths[ci] = measure(ci);
        }
    }
    if reset {
        state.widths = cols.iter().map(|c| c.width).collect();
    }
}

/// Row indices in display order.
fn sorted(rows: &[Vec<Cell>], sort: Option<(usize, bool)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    if let Some((ci, desc)) = sort {
        order.sort_by(|&a, &b| {
            let (x, y) = (rows[a].get(ci), rows[b].get(ci));
            let o = match (x.and_then(|c| c.num), y.and_then(|c| c.num)) {
                (Some(m), Some(n)) => m.partial_cmp(&n).unwrap_or(Ordering::Equal),
                _ => {
                    let t = |c: Option<&Cell>| c.map(|c| c.text.to_lowercase()).unwrap_or_default();
                    t(x).cmp(&t(y))
                }
            };
            if desc { o.reverse() } else { o }
        });
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_numbers_and_text() {
        let rows = vec![
            vec![Cell::text("b"), Cell::num(10.0, "10")],
            vec![Cell::text("A"), Cell::num(2.0, "2")],
            vec![Cell::text("c"), Cell::num(33.0, "33")],
        ];
        assert_eq!(sorted(&rows, None), [0, 1, 2]);
        assert_eq!(sorted(&rows, Some((0, false))), [1, 0, 2], "text sorts A to Z, ignoring case");
        assert_eq!(sorted(&rows, Some((1, true))), [2, 0, 1], "numbers sort by value, not as text");
    }
}
