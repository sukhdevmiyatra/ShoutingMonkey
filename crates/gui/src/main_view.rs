//! The centre of the window: report tabs, the filter / export / search
//! row, and the results table.

use egui::{Align, CornerRadius, FontId, Layout, RichText, Sense, Stroke, vec2};

use crate::app::{App, DetailTab, fmt_count, next_sort};
use crate::model::{self, VIEWS};
use crate::query::{self, ROW_LIMIT};
use crate::theme::{self, Palette};

const ROW_HEIGHT: f32 = 24.0;
const HEADER_HEIGHT: f32 = 28.0;
/// Horizontal space between columns.
const CELL_GAP: f32 = 8.0;

/// A tab in a tab strip: the selected one joins the surface below it.
pub fn tab_button(ui: &mut egui::Ui, p: &Palette, label: &str, selected: bool) -> egui::Response {
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, p.text);
    let size = vec2(galley.size().x + 22.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
        if selected {
            painter.rect_filled(rect, radius, p.surface);
            painter.rect_stroke(rect, radius, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            // Hide the bottom border so the tab opens onto the content.
            painter.line_segment([rect.left_bottom() + vec2(1.0, -0.5), rect.right_bottom() + vec2(-1.0, -0.5)], Stroke::new(1.5, p.surface));
            painter.line_segment([rect.left_top() + vec2(3.0, 0.5), rect.right_top() + vec2(-3.0, 0.5)], Stroke::new(2.0, p.accent));
        } else if resp.hovered() {
            painter.rect_filled(rect, radius, p.hover);
        }
        let color = if selected { p.text } else { p.muted };
        painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    }
    resp
}

impl App {
    fn count(&self, tab: usize, filter: usize) -> Option<u64> {
        self.counts.as_ref().map(|c| c.filters[model::flat_index(tab, filter)])
    }

    pub fn main_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        // The view's name and what it holds; the Navigator picks the view.
        let view = &VIEWS[self.tab];
        ui.horizontal(|ui| {
            ui.label(RichText::new(view.title).family(theme::semibold()).size(18.0));
            ui.label(RichText::new(view.section.label()).color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.prefs.show_navigator && ui.small_button("Show navigator").clicked() {
                    self.prefs.show_navigator = true;
                    self.prefs_dirty = true;
                }
                let problems = view.filters.iter().enumerate().filter(|(_, f)| f.problem.is_some()).filter_map(|(fi, _)| self.count(self.tab, fi)).filter(|n| *n > 0).count();
                if problems > 0 {
                    ui.label(RichText::new(format!("{problems} {} in this tab", if problems == 1 { "issue" } else { "issues" })).color(p.muted));
                }
            });
        });
        ui.add_space(4.0);

        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            self.filter_row(ui, p);
            ui.add_space(4.0);
            self.table(ui, p);
        });
    }

    fn filter_row(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let tab = &VIEWS[self.tab];
        ui.horizontal(|ui| {
            let label = |fi: usize| match self.count(self.tab, fi) {
                Some(n) => format!("{} ({})", tab.filters[fi].label, fmt_count(n)),
                None => tab.filters[fi].label.to_string(),
            };
            let current = label(self.filter);
            let mut chosen = None;
            let caption = ui.label("Filter");
            egui::ComboBox::from_id_salt("filter")
                .width(280.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for fi in 0..tab.filters.len() {
                        if ui.selectable_label(fi == self.filter, label(fi)).clicked() {
                            chosen = Some(fi);
                        }
                    }
                })
                .response
                .labelled_by(caption.id);
            if let Some(fi) = chosen {
                self.filter = fi;
            }
            if ui.add_enabled(self.db.is_some(), egui::Button::new("Export")).on_hover_text("Save this tab and filter as CSV").clicked() {
                self.export_current_view();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let search = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search").desired_width(240.0));
                if std::mem::take(&mut self.focus_search) {
                    search.request_focus();
                }
                if !self.search.is_empty() && ui.small_button("Clear").clicked() {
                    self.search.clear();
                }
                let shown = fmt_count(self.rows.len() as u64);
                let text = if self.rows_truncated {
                    format!("First {} shown; Export for all", fmt_count(ROW_LIMIT as u64))
                } else if !self.search.trim().is_empty() {
                    format!("{shown} matching")
                } else {
                    format!("{shown} URLs")
                };
                ui.label(RichText::new(text).color(p.muted));
            });
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let tab = &VIEWS[self.tab];
        let visible: Vec<usize> = (0..tab.columns.len()).filter(|&i| i == 0 || !self.prefs.is_hidden(tab.id, tab.columns[i])).collect();
        let widths: Vec<f32> = visible.iter().map(|&ci| self.prefs.column_width(tab.id, tab.columns[ci])).collect();
        let total_w: f32 = widths.iter().map(|w| w + CELL_GAP).sum();
        // Rows loaded for another tab hold other columns: show none until
        // this tab's rows arrive.
        let row_count = if self.rows_tab == Some(self.tab) { self.rows.len() } else { 0 };

        let mut clicked: Option<String> = None;
        let mut action: Option<(String, RowAction)> = None;

        // The header is reserved first and painted after the body, so it
        // follows the body's horizontal scroll in the same frame.
        let (header_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());

        let mut area = egui::ScrollArea::both().id_salt(("table", tab.id)).auto_shrink([false, false]);
        if let Some(i) = self.scroll_to.take() {
            let (offset, view) = self.table_scroll;
            let top = i as f32 * ROW_HEIGHT;
            if top < offset {
                area = area.vertical_scroll_offset(top);
            } else if top + ROW_HEIGHT > offset + view {
                area = area.vertical_scroll_offset(top + ROW_HEIGHT - view);
            }
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let out = area.show_rows(ui, ROW_HEIGHT, row_count, |ui, range| {
            ui.set_min_width(total_w);
            for index in range {
                let r = &self.rows[index];
                let (rect, _) = ui.allocate_exact_size(vec2(total_w.max(ui.available_width()), ROW_HEIGHT), Sense::hover());
                let resp = ui.interact(rect, ui.scope_id().with(("row", index)), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, &r.url));
                let selected = self.selected.as_deref() == Some(r.url.as_str());
                let fill = if selected {
                    Some(p.selection)
                } else if resp.hovered() {
                    Some(p.hover)
                } else if index % 2 == 1 {
                    Some(p.zebra)
                } else {
                    None
                };
                if let Some(f) = fill {
                    ui.painter().rect_filled(rect, 0.0, f);
                }
                let mut x = rect.left() + CELL_GAP / 2.0;
                for (k, &ci) in visible.iter().enumerate() {
                    let cell_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(widths[k], ROW_HEIGHT));
                    x += widths[k] + CELL_GAP;
                    if !ui.is_rect_visible(cell_rect) {
                        continue;
                    }
                    let mut cell_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect).layout(Layout::left_to_right(Align::Center)));
                    cell_ui.set_clip_rect(cell_rect.intersect(ui.clip_rect()));
                    cell(&mut cell_ui, p, &r.cells[ci], r.status, tab.columns[ci]);
                }
                if resp.clicked() || resp.secondary_clicked() {
                    clicked = Some(r.url.clone());
                }
                resp.context_menu(|ui| {
                    for (label, a) in [
                        ("Copy URL", RowAction::Copy),
                        ("Open in Browser", RowAction::Open),
                        ("Show Inlinks", RowAction::Inlinks),
                        ("Show Outlinks", RowAction::Outlinks),
                        ("Show SERP Snippet", RowAction::Serp),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some((r.url.clone(), a));
                            ui.close();
                        }
                    }
                });
            }
        });
        self.table_scroll = (out.state.clamped_offset().y, out.inner_rect.height());
        self.table_rect = out.inner_rect;

        // An empty table says why, in its first row, as desktop tables do.
        if row_count == 0 && self.db.is_some() {
            let message = if self.rows_tab != Some(self.tab) || (self.running() && self.rows.is_empty()) {
                "Loading…"
            } else if !self.search.trim().is_empty() {
                "No URLs match the search."
            } else {
                "No URLs in this filter."
            };
            ui.painter().text(out.inner_rect.left_top() + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, message, FontId::proportional(13.0), p.muted);
        }

        self.table_header(ui, p, header_rect, &visible, &widths, out.state.clamped_offset().x);

        if clicked.is_some() {
            self.select(clicked);
        }
        if let Some((url, a)) = action {
            self.select(Some(url.clone()));
            match a {
                RowAction::Copy => ui.ctx().copy_text(url),
                RowAction::Open => ui.ctx().open_url(egui::OpenUrl::new_tab(&url)),
                RowAction::Inlinks => self.show_detail(DetailTab::Inlinks),
                RowAction::Outlinks => self.show_detail(DetailTab::Outlinks),
                RowAction::Serp => self.show_detail(DetailTab::Serp),
            }
        }
    }

    /// Column headings: click to sort, drag an edge to resize, right-click
    /// to choose columns. They scroll sideways with the body.
    fn table_header(&mut self, ui: &mut egui::Ui, p: &Palette, rect: egui::Rect, visible: &[usize], widths: &[f32], offset_x: f32) {
        let tab = &VIEWS[self.tab];
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.table_head);
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, p.border));
        let font = FontId::new(13.5, theme::semibold());
        let mut x = rect.left() - offset_x;
        let mut sort_click = None;
        let mut toggle_col: Option<(&'static str, bool)> = None;
        let mut reset = false;
        for (k, &ci) in visible.iter().enumerate() {
            let id = tab.columns[ci];
            let c = model::col(id);
            let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(widths[k] + CELL_GAP, rect.height()));
            x += widths[k] + CELL_GAP;
            if cell.right() < rect.left() || cell.left() > rect.right() {
                continue;
            }
            let hit = cell.intersect(rect);
            let resp = ui.interact(hit, ui.scope_id().with(("head", tab.id, id)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColumnHeader, true, c.title));
            if resp.hovered() {
                painter.rect_filled(hit, 0.0, p.hover);
            }
            let arrow = match self.sort {
                Some(s) if s.column == id && s.descending => " ▼",
                Some(s) if s.column == id => " ▲",
                _ => "",
            };
            let galley = painter.layout_no_wrap(format!("{}{arrow}", c.title), font.clone(), p.text);
            let inner = cell.shrink2(vec2(CELL_GAP / 2.0, 0.0));
            let tx = if c.numeric { (inner.right() - galley.size().x).max(inner.left()) } else { inner.left() };
            painter
                .with_clip_rect(inner.intersect(rect))
                .galley(egui::pos2(tx, cell.center().y - galley.size().y / 2.0), galley, p.text);
            painter.line_segment([cell.right_top() + vec2(0.0, 5.0), cell.right_bottom() - vec2(0.0, 5.0)], Stroke::new(1.0, p.border));
            if resp.clicked() {
                sort_click = Some(id);
            }
            resp.on_hover_text(format!("{}\nClick to sort, drag the edge to resize, right-click to choose columns.", c.title))
                .context_menu(|ui| {
                    ui.label(RichText::new("Columns").color(p.muted).small());
                    for &cj in tab.columns.iter().skip(1) {
                        let mut shown = !self.prefs.is_hidden(tab.id, cj);
                        if ui.checkbox(&mut shown, model::col(cj).title).changed() {
                            toggle_col = Some((cj, !shown));
                        }
                    }
                    ui.separator();
                    if ui.button("Reset Columns").clicked() {
                        reset = true;
                        ui.close();
                    }
                });
            // Resize handle on the right edge.
            let handle = egui::Rect::from_center_size(egui::pos2(cell.right(), cell.center().y), vec2(8.0, rect.height()));
            let h = ui.interact(handle.intersect(rect), ui.scope_id().with(("resize", tab.id, id)), Sense::drag());
            if h.hovered() || h.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if h.dragged() {
                self.prefs.set_column_width(tab.id, id, (widths[k] + h.drag_delta().x).clamp(40.0, 1600.0));
            }
            if h.drag_stopped() {
                self.prefs_dirty = true;
            }
        }
        if let Some(id) = sort_click {
            self.sort = next_sort(self.sort, id, query::is_numeric(id));
        }
        if let Some((col, hidden)) = toggle_col {
            self.prefs.set_hidden(tab.id, col, hidden);
            self.prefs_dirty = true;
        }
        if reset {
            self.prefs.reset_columns(tab.id);
            self.prefs_dirty = true;
        }
    }

    fn show_detail(&mut self, tab: DetailTab) {
        self.detail_tab = tab;
        self.prefs.show_inspector = true;
    }
}

#[derive(Clone, Copy)]
enum RowAction {
    Copy,
    Open,
    Inlinks,
    Outlinks,
    Serp,
}

fn cell(ui: &mut egui::Ui, p: &Palette, text: &str, status: Option<i64>, column: &str) {
    if column == "status_code" {
        let color = theme::status_color(p, status);
        let (chip, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().rect_filled(chip, 2.0, color);
        ui.label(if text.is_empty() { "0" } else { text });
        return;
    }
    if column == "indexability" && text == "Non-Indexable" {
        ui.label(RichText::new(text).color(p.danger));
        return;
    }
    if query::is_numeric(column) {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(4.0);
            ui.label(text);
        });
    } else {
        ui.add(egui::Label::new(text).truncate());
    }
}
