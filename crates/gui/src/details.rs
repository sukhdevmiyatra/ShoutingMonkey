//! The bottom panel: everything about the selected URL.

use egui::{Align, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, vec2};
use engine::store::{Table, value_text};
use rusqlite::types::Value;

use crate::app::{App, DetailTab, fmt_count};
use crate::grid::{Cell, GridCol, grid};
use crate::main_view::tab_button;
use crate::query::Serp;
use crate::theme::{self, Palette};

/// Desktop result widths search engines truncate at.
const TITLE_LIMIT_PX: u32 = 580;
const DESCRIPTION_LIMIT_PX: u32 = 990;

impl App {
    pub fn details_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        egui::ScrollArea::horizontal().id_salt("detail-tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                for t in DetailTab::ALL {
                    let label = match (&self.detail, t) {
                        (Some(d), DetailTab::Inlinks) => format!("Inlinks ({})", fmt_count(d.inlinks.rows.len() as u64)),
                        (Some(d), DetailTab::Outlinks) => format!("Outlinks ({})", fmt_count(d.outlinks.rows.len() as u64)),
                        _ => t.label().to_string(),
                    };
                    if tab_button(ui, p, &label, self.detail_tab == t).clicked() {
                        self.detail_tab = t;
                    }
                }
            });
        });
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            let Some(url) = self.selected.clone() else {
                ui.label(RichText::new("Select a URL in the table above to see its details.").color(p.muted));
                return;
            };
            let Some(d) = &self.detail else {
                ui.label(RichText::new("Loading…").color(p.muted));
                return;
            };
            let mut go_to = None;
            match self.detail_tab {
                DetailTab::UrlDetails => {
                    name_values(ui, p, "fields", ("Name", "Value"), &d.fields);
                }
                DetailTab::Inlinks => go_to = link_table(ui, p, "inlinks", &d.inlinks, Some(4)),
                DetailTab::Outlinks => go_to = link_table(ui, p, "outlinks", &d.outlinks, Some(4)),
                DetailTab::Images => go_to = link_table(ui, p, "images", &d.images, Some(2)),
                DetailTab::Resources => go_to = link_table(ui, p, "resources", &d.resources, Some(2)),
                DetailTab::Duplicates => go_to = link_table(ui, p, "dupes", &d.duplicates, None),
                DetailTab::Serp => serp_snippet(ui, p, &url, &d.serp),
                DetailTab::Headers => {
                    if d.headers.is_empty() {
                        ui.label(RichText::new("No headers were recorded for this URL.").color(p.muted));
                    } else {
                        name_values(ui, p, "headers", ("Header", "Value"), &d.headers);
                    }
                }
                DetailTab::Source => match &d.source {
                    Some(html) => {
                        egui::ScrollArea::both().id_salt("source").auto_shrink([false, false]).show(ui, |ui| {
                            let mut text = html.as_str();
                            ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).code_editor());
                        });
                    }
                    None => {
                        ui.label(RichText::new("Page HTML was not stored for this crawl.").color(p.muted));
                        if ui.button("Turn on Store HTML…").clicked() {
                            self.config_dialog.open_at(crate::dialogs::ConfigPage::Advanced, &self.cfg);
                        }
                    }
                },
            }
            if let Some(u) = go_to {
                self.select(Some(u));
            }
        });
    }
}

/// Name and value rows filling the width; long values wrap and can be
/// selected for copying.
fn name_values(ui: &mut egui::Ui, p: &Palette, id: &str, head: (&str, &str), rows: &[(String, String)]) {
    const NAME_W: f32 = 230.0;
    let row = |ui: &mut egui::Ui, k: RichText, v: egui::Label, fill: Option<Color32>| {
        let frame = egui::Frame::new().fill(fill.unwrap_or(Color32::TRANSPARENT)).inner_margin(egui::Margin::symmetric(6, 3));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(NAME_W, 18.0), Layout::left_to_right(Align::Min), |ui| {
                    ui.set_width(NAME_W);
                    ui.add(egui::Label::new(k).truncate());
                });
                ui.add(v.wrap());
            });
        });
    };
    row(ui, RichText::new(head.0).family(theme::semibold()), egui::Label::new(RichText::new(head.1).family(theme::semibold())), None);
    ui.separator();
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (k, v)) in rows.iter().enumerate() {
            row(ui, RichText::new(k).color(p.muted), egui::Label::new(v.as_str()).selectable(true), (i % 2 == 1).then_some(p.zebra));
        }
    });
}

/// A table of links; the first column holds URLs: click one to select it.
/// `status_col` marks a column of HTTP statuses to colour.
fn link_table(ui: &mut egui::Ui, p: &Palette, id: &str, t: &Table, status_col: Option<usize>) -> Option<String> {
    if t.rows.is_empty() {
        ui.label(RichText::new("None found.").color(p.muted));
        return None;
    }
    let numeric: Vec<bool> = (0..t.columns.len())
        .map(|ci| t.rows.iter().any(|r| matches!(r.get(ci), Some(Value::Integer(_) | Value::Real(_)))))
        .collect();
    let cols: Vec<GridCol> = t
        .columns
        .iter()
        .enumerate()
        .map(|(ci, c)| GridCol { title: c, width: if ci == 0 { 460.0 } else { 120.0 }, numeric: numeric[ci] })
        .collect();
    let rows: Vec<Vec<Cell>> = t
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .enumerate()
                .map(|(ci, v)| {
                    let text = value_text(v);
                    let num = match v {
                        Value::Integer(i) => Some(*i as f64),
                        Value::Real(f) => Some(*f),
                        _ => None,
                    };
                    let cell = Cell { num, ..Cell::text(text.clone()) };
                    if ci == 0 {
                        cell.color(p.accent)
                    } else if Some(ci) == status_col {
                        match num {
                            Some(s) => cell.chip(theme::status_color(p, Some(s as i64))),
                            None => Cell::text("Not crawled").color(p.muted),
                        }
                    } else {
                        cell
                    }
                })
                .collect()
        })
        .collect();
    let resp = grid(ui, p, id, &cols, &rows, None);
    let row = resp.clicked.filter(|(_, ci)| *ci == 0).map(|(ri, _)| ri).or(resp.double_clicked)?;
    Some(value_text(&t.rows[row][0]))
}

/// Cut `text` so its estimated width fits `limit` px, adding an ellipsis.
fn fit(text: &str, size: f32, limit: u32) -> String {
    if engine::parse::pixel_width(text, size) <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if engine::parse::pixel_width(&format!("{out} ..."), size) > limit {
            out.pop();
            break;
        }
    }
    format!("{} ...", out.trim_end())
}

/// A preview of how the page may appear as a desktop search result.
fn serp_snippet(ui: &mut egui::Ui, p: &Palette, url: &str, s: &Serp) {
    let dark = ui.visuals().dark_mode;
    let (title_color, url_color) = if dark {
        (Color32::from_rgb(0x8A, 0xB4, 0xF8), Color32::from_rgb(0xBD, 0xC1, 0xC6))
    } else {
        (Color32::from_rgb(0x1A, 0x0D, 0xAB), Color32::from_rgb(0x4D, 0x51, 0x56))
    };
    // Preview and meters side by side when they fit, stacked when not; the
    // area scrolls both ways for anything narrower still.
    let layout = if ui.available_width() >= 950.0 { Layout::left_to_right(Align::Min) } else { Layout::top_down(Align::Min) };
    egui::ScrollArea::both().id_salt("serp").auto_shrink([false, false]).show(ui, |ui| {
        ui.with_layout(layout, |ui| {
            egui::Frame::new()
                .fill(if dark { Color32::from_rgb(0x20, 0x21, 0x24) } else { Color32::WHITE })
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(600.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    let parsed = url::Url::parse(url).ok();
                    let host = parsed.as_ref().and_then(|u| u.host_str()).unwrap_or("").to_string();
                    let crumbs: Vec<String> = parsed
                        .as_ref()
                        .map(|u| u.path_segments().map(|s| s.filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default())
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 13.0, p.hover);
                        let initial = host.trim_start_matches("www.").chars().next().unwrap_or('?').to_ascii_uppercase();
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, initial, FontId::proportional(13.0), p.text);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(host.trim_start_matches("www.")).size(14.0).color(if dark { Color32::WHITE } else { Color32::from_rgb(0x20, 0x21, 0x24) }));
                            let mut path = format!("https://{host}");
                            for c in crumbs.iter().take(3) {
                                path.push_str(" > ");
                                path.push_str(c);
                            }
                            ui.label(RichText::new(path).size(12.0).color(url_color));
                        });
                    });
                    ui.add_space(4.0);
                    let title = if s.title.is_empty() { "(no title)".to_string() } else { fit(&s.title, 20.0, TITLE_LIMIT_PX) };
                    ui.label(RichText::new(title).size(20.0).color(title_color));
                    ui.add_space(2.0);
                    let desc = if s.description.is_empty() {
                        "No meta description: search engines will choose text from the page.".to_string()
                    } else {
                        fit(&s.description, 14.0, DESCRIPTION_LIMIT_PX)
                    };
                    ui.add(egui::Label::new(RichText::new(desc).size(14.0).color(url_color)).wrap());
                    });
                });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(300.0);
                ui.label(RichText::new("Fit in desktop results").family(theme::semibold()));
                ui.add_space(4.0);
                meter(ui, p, "Title", s.title_px, TITLE_LIMIT_PX, s.title.chars().count());
                meter(ui, p, "Description", s.description_px, DESCRIPTION_LIMIT_PX, s.description.chars().count());
                ui.add_space(6.0);
                ui.add(egui::Label::new(RichText::new("Widths are estimated from the fonts search results use; the preview is cut off the same way.").small().color(p.muted)).wrap());
            });
        });
    });
}

fn meter(ui: &mut egui::Ui, p: &Palette, label: &str, px: u32, limit: u32, chars: usize) {
    let over = px > limit;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let text = format!("{px} of {limit} px, {chars} characters");
            ui.label(RichText::new(text).color(if over { p.danger } else { p.muted }));
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, p.hover);
    let share = (px as f32 / limit as f32).clamp(0.0, 1.0);
    if share > 0.0 {
        let filled = egui::Rect::from_min_size(rect.min, vec2(rect.width() * share, rect.height()));
        ui.painter().rect_filled(filled, 4.0, if over { p.danger } else { p.series });
    }
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("Short", 20.0, 580), "Short");
        let long = "A very long title that keeps going well past the point where any search engine would cut it off";
        let cut = fit(long, 20.0, 580);
        assert!(cut.ends_with(" ..."), "{cut}");
        assert!(engine::parse::pixel_width(&cut, 20.0) <= 580);
    }
}
