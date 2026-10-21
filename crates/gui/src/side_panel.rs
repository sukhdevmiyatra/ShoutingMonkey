//! The Navigator (left: every view and filter with counts) and the Insights
//! panel (right: Problems, Breakdown, Structure, Speed, Changes).

use egui::{RichText, Sense, Stroke, vec2};

use crate::app::{App, InsightTab, fmt_count};
use crate::charts::{self, Bar};
use crate::grid::{Cell, GridCol, grid};
use crate::main_view::tab_button;
use crate::model::{self, Section, Severity, VIEWS};
use crate::query::percent;
use crate::theme::{self, Palette};

impl App {
    fn count_of(&self, view: usize, filter: usize) -> Option<u64> {
        self.counts.as_ref().map(|c| c.filters[model::flat_index(view, filter)])
    }

    /// Left panel: sections, views and their filters, each with a count.
    pub fn navigator(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let mut go = None;
        if let Some(c) = &self.counts {
            ui.label(RichText::new(format!("{} URLs in this crawl", fmt_count(c.total))).color(p.muted).small());
        }
        egui::ScrollArea::vertical().id_salt("navigator").auto_shrink([false, false]).show(ui, |ui| {
            for section in Section::ALL {
                ui.add_space(8.0);
                ui.label(RichText::new(section.label()).family(theme::semibold()).color(p.section));
                ui.add_space(2.0);
                for (vi, view) in VIEWS.iter().enumerate().filter(|(_, v)| v.section == section) {
                    let expanded = self.nav_expanded.contains(&vi);
                    let count = self.count_of(vi, 0);
                    let problems: u64 = view
                        .filters
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.problem.as_ref().is_some_and(|p| p.severity == Severity::Error))
                        .filter_map(|(fi, _)| self.count_of(vi, fi))
                        .sum();
                    let selected = self.tab == vi && (self.filter == 0 || !expanded);
                    let resp = tree_row(ui, p, view.title, count, None, Some(expanded), selected, true);
                    if problems > 0 {
                        draw_badge(ui, &resp, p, problems);
                    }
                    if resp.clicked() {
                        let on_arrow = resp.interact_pointer_pos().is_some_and(|q| q.x < resp.rect.left() + 22.0);
                        if on_arrow && expanded {
                            self.nav_expanded.remove(&vi);
                        } else {
                            self.nav_expanded.insert(vi);
                        }
                        if !on_arrow {
                            go = Some((vi, 0));
                        }
                    }
                    if !expanded {
                        continue;
                    }
                    let all = count.unwrap_or(0);
                    for (fi, f) in view.filters.iter().enumerate().skip(1) {
                        let n = self.count_of(vi, fi);
                        let share = n.map(|n| percent(n, all));
                        let resp = tree_row(ui, p, f.label, n, share, None, self.tab == vi && self.filter == fi, false);
                        let resp = match &f.problem {
                            Some(pr) => {
                                if n.unwrap_or(0) > 0 {
                                    draw_marker(ui, &resp, theme::issue_color(p, pr.severity));
                                }
                                resp.on_hover_text(format!("{} ({} impact)\n{}", pr.severity.label(), pr.impact.label().to_lowercase(), pr.about))
                            }
                            None => resp,
                        };
                        if resp.clicked() {
                            go = Some((vi, fi));
                        }
                    }
                }
            }
            ui.add_space(8.0);
        });
        if let Some((v, f)) = go {
            self.go_to(v, f);
        }
    }

    pub fn insights_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for t in InsightTab::ALL {
                if tab_button(ui, p, t.label(), self.insight_tab == t).clicked() {
                    self.insight_tab = t;
                }
            }
        });
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if self.db.is_none() {
                ui.label(RichText::new("Start or open a crawl to see insights.").color(p.muted));
                return;
            }
            match self.insight_tab {
                InsightTab::Problems => self.problems(ui, p),
                InsightTab::Breakdown => self.breakdown(ui, p),
                InsightTab::Structure => self.structure(ui, p),
                InsightTab::Speed => self.speed(ui, p),
                InsightTab::Changes => self.changes(ui, p),
            }
        });
    }

    fn breakdown(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(counts) = &self.counts else {
            ui.label(RichText::new("Counting…").color(p.muted));
            return;
        };
        let vi = self.tab;
        let view = &VIEWS[vi];
        let all = counts.filters[model::flat_index(vi, 0)];
        ui.label(RichText::new(view.title).family(theme::semibold()));
        ui.label(RichText::new(format!("{} URLs, split by filter. Click a bar to show it.", fmt_count(all))).color(p.muted).small());
        ui.add_space(4.0);
        let bars: Vec<Bar> = view
            .filters
            .iter()
            .enumerate()
            .skip(1)
            .map(|(fi, f)| Bar {
                label: f.label,
                value: counts.filters[model::flat_index(vi, fi)],
                color: f.problem.as_ref().map(|pr| theme::issue_color(p, pr.severity)),
            })
            .collect();
        let mut go = None;
        egui::ScrollArea::vertical().id_salt("breakdown").auto_shrink([false, false]).show(ui, |ui| {
            go = charts::bar_list(ui, p, &bars, all);
            ui.add_space(10.0);
            ui.label(RichText::new("Bars in a severity colour are problems; blue bars are plain groupings.").small().color(p.muted));
        });
        if let Some(i) = go {
            self.go_to(vi, i + 1);
        }
    }

    fn problems(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(counts) = &self.counts else {
            ui.label(RichText::new("Counting…").color(p.muted));
            return;
        };
        let mut found: Vec<(usize, usize, &model::Problem, u64, u64)> = model::problems()
            .map(|(vi, fi, pr)| (vi, fi, pr, counts.filters[model::flat_index(vi, fi)], counts.filters[model::flat_index(vi, 0)]))
            .filter(|(.., n, _)| *n > 0)
            .collect();
        found.sort_by_key(|a| (a.2.severity, a.2.impact, std::cmp::Reverse(a.3)));
        if let Some(stats) = &self.stats
            && !stats.notes.is_empty()
        {
            egui::CollapsingHeader::new(format!("Crawl notes ({})", stats.notes.len())).id_salt("crawl-notes").show(ui, |ui| {
                for n in &stats.notes {
                    ui.add(egui::Label::new(RichText::new(n).small()).wrap());
                }
            });
        }
        // Click a severity to list only those; click it again for all.
        ui.horizontal_wrapped(|ui| {
            for s in Severity::ALL {
                let n = found.iter().filter(|f| f.2.severity == s).count();
                let on = self.issue_severity.is_none_or(|v| v == s);
                let text = RichText::new(format!("{n} {}", if n == 1 { s.label() } else { s.plural() })).color(if on { p.text } else { p.muted });
                let resp = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, if on { theme::issue_color(p, s) } else { p.border });
                        ui.add(egui::Button::selectable(self.issue_severity == Some(s), text).frame_when_inactive(false))
                    })
                    .inner
                    .on_hover_text(format!("Show only {}", s.plural().to_lowercase()));
                if resp.clicked() {
                    self.issue_severity = if self.issue_severity == Some(s) { None } else { Some(s) };
                }
            }
        });
        if let Some(s) = self.issue_severity {
            found.retain(|f| f.2.severity == s);
        }
        ui.separator();
        if found.is_empty() {
            ui.label(RichText::new(if self.running() { "No issues found yet." } else { "No issues found." }).color(p.muted));
            return;
        }
        let desc_h = 150.0;
        let table_h = (ui.available_height() - desc_h - 10.0).max(120.0);
        let rows: Vec<Vec<Cell>> = found
            .iter()
            .map(|&(vi, fi, pr, n, all)| {
                vec![
                    Cell::text(model::problem_name(vi, fi)),
                    Cell::num(pr.severity as u8 as f64, pr.severity.label()).chip(theme::issue_color(p, pr.severity)),
                    Cell::num(pr.impact as u8 as f64, pr.impact.label()),
                    Cell::num(n as f64, fmt_count(n)),
                    Cell::num(n as f64 / all.max(1) as f64, percent(n, all)),
                ]
            })
            .collect();
        let selected = found.iter().position(|f| Some((f.0, f.1)) == self.selected_problem);
        let cols = [
            GridCol::text("Issue Name", (ui.available_width() - 282.0).max(140.0)),
            GridCol::text("Issue Type", 88.0),
            GridCol::text("Priority", 64.0),
            GridCol::num("URLs", 54.0),
            GridCol::num("% of Total", 74.0),
        ];
        let resp = ui.allocate_ui(vec2(ui.available_width(), table_h), |ui| grid(ui, p, "issues", &cols, &rows, selected)).inner;
        let clicked = resp.clicked.map(|(i, _)| (found[i].0, found[i].1));
        if let Some((vi, fi)) = clicked {
            self.selected_problem = Some((vi, fi));
            self.go_to(vi, fi);
        }
        ui.separator();
        egui::ScrollArea::vertical().id_salt("problem-desc").auto_shrink([false, false]).show(ui, |ui| {
            match self.selected_problem.and_then(|(vi, fi)| VIEWS[vi].filters[fi].problem.as_ref().map(|pr| (vi, fi, pr))) {
                Some((vi, fi, pr)) => {
                    ui.label(RichText::new(model::problem_name(vi, fi)).family(theme::semibold()));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Description").color(p.muted).small());
                    ui.label(pr.about);
                    ui.add_space(4.0);
                    ui.label(RichText::new("How To Fix").color(p.muted).small());
                    ui.label(pr.fix);
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Check id: {}", VIEWS[vi].filters[fi].slug)).small().color(p.muted));
                }
                None => {
                    ui.label(RichText::new("Select an issue to see what it means and how to fix it.").color(p.muted));
                }
            }
        });
    }

    fn structure(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(stats) = &self.stats else {
            ui.label(RichText::new("Measuring…").color(p.muted));
            return;
        };
        ui.label(RichText::new("Clicks from the start page").family(theme::semibold()));
        ui.label(RichText::new("Your URLs by crawl depth").color(p.muted).small());
        let labels: Vec<String> = (0..=10).map(|d| if d == 10 { "10+".to_string() } else { d.to_string() }).collect();
        let bars: Vec<Bar> = (0..=10u32)
            .map(|d| Bar { label: &labels[d as usize], value: stats.depth.iter().find(|(k, _)| *k == d).map_or(0, |(_, v)| *v), color: None })
            .collect();
        charts::columns(ui, p, &bars, 170.0, "clicks deep");
        if stats.sitemap_only > 0 {
            ui.label(
                RichText::new(format!("{} more URLs were found only in the XML sitemap and have no crawl depth.", fmt_count(stats.sitemap_only)))
                    .small()
                    .color(p.muted),
            );
        }
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Folders").family(theme::semibold()));
        ui.label(RichText::new("Your web pages per folder").color(p.muted).small());
        let total: u64 = stats.directories.iter().map(|d| d.count).sum();
        egui::ScrollArea::vertical().id_salt("dirs").auto_shrink([false, false]).show(ui, |ui| {
            for d in &stats.directories {
                let id = ui.scope_id().with(("dir", &d.path));
                let open = !d.children.is_empty() && ui.data(|m| m.get_temp::<bool>(id)).unwrap_or(false);
                let expander = (!d.children.is_empty()).then_some(open);
                let resp = tree_row(ui, p, &d.path, Some(d.count), Some(percent(d.count, total)), expander, false, false);
                if resp.clicked() && expander.is_some() {
                    ui.data_mut(|m| m.insert_temp(id, !open));
                }
                if open {
                    for c in &d.children {
                        tree_row(ui, p, &c.path, Some(c.count), Some(percent(c.count, d.count)), None, false, false);
                    }
                }
            }
        });
    }

    fn speed(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(stats) = &self.stats else {
            ui.label(RichText::new("Measuring…").color(p.muted));
            return;
        };
        ui.label(RichText::new("Time to respond").family(theme::semibold()));
        ui.label(RichText::new("Your web pages by response time, in seconds").color(p.muted).small());
        let labels: Vec<String> = (0..=10).map(|s| if s == 10 { "10+".into() } else { format!("{s}–{}", s + 1) }).collect();
        let bars: Vec<Bar> = stats.response_times.iter().enumerate().map(|(i, v)| Bar { label: &labels[i], value: *v, color: None }).collect();
        charts::columns(ui, p, &bars, 190.0, "s");
        ui.add_space(6.0);
        let total: u64 = stats.response_times.iter().sum();
        egui::Grid::new("rt").num_columns(3).striped(true).spacing([24.0, 3.0]).show(ui, |ui| {
            ui.label(RichText::new("Response time").family(theme::semibold()));
            ui.label(RichText::new("Pages").family(theme::semibold()));
            ui.label(RichText::new("Share").family(theme::semibold()));
            ui.end_row();
            for (i, v) in stats.response_times.iter().enumerate() {
                ui.label(if i == 10 { "10 s and over".to_string() } else { format!("{i} to {} s", i + 1) });
                ui.label(fmt_count(*v));
                ui.label(percent(*v, total));
                ui.end_row();
            }
        });
    }

    fn changes(&mut self, ui: &mut egui::Ui, p: &Palette) {
        if self.running() {
            ui.label(RichText::new("Changes are worked out once the crawl finishes.").color(p.muted));
            return;
        }
        // Listed once per crawl shown, not every frame.
        if self.baseline_candidates.is_none() {
            self.baseline_candidates = Some(crate::paths::earlier_crawls(self.db.as_deref()));
        }
        let candidates = self.baseline_candidates.clone().unwrap_or_default();
        if self.baseline.is_none()
            && let Some(first) = candidates.first()
        {
            self.set_baseline(first.clone());
        }
        ui.horizontal(|ui| {
            ui.label("Compared with");
            let current = self.baseline.as_ref().map(|b| crate::paths::crawl_name(b)).unwrap_or_else(|| "no earlier crawl".into());
            let mut pick = None;
            let caption = ui.label("");
            egui::ComboBox::from_id_salt("baseline")
                .width(240.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for c in &candidates {
                        if ui.selectable_label(Some(c) == self.baseline.as_ref(), crate::paths::crawl_name(c)).clicked() {
                            pick = Some(c.clone());
                        }
                    }
                    ui.separator();
                    if ui.button("Another file…").clicked() {
                        pick = rfd::FileDialog::new().add_filter("Crawl file", &["db"]).set_directory(crate::paths::crawls_dir()).pick_file();
                    }
                })
                .response
                .labelled_by(caption.id);
            if let Some(b) = pick {
                self.set_baseline(b);
            }
        });
        if self.baseline.is_none() {
            ui.add_space(6.0);
            ui.label(RichText::new("Crawl the same site again later, and this tab will show which problems are new and which were fixed.").color(p.muted));
            return;
        }
        let Some(cmp) = &self.comparison else {
            ui.label(RichText::new(if self.comparing { "Comparing…" } else { "" }).color(p.muted));
            return;
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(format!("{} new pages", fmt_count(cmp.pages_added)));
            ui.separator();
            ui.label(format!("{} gone", fmt_count(cmp.pages_removed)));
            ui.separator();
            let (new, fixed): (usize, usize) = cmp.problems.iter().map(|d| (d.new.len(), d.fixed.len())).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            ui.label(RichText::new(format!("{} new problems", fmt_count(new as u64))).color(if new > 0 { p.danger } else { p.text }));
            ui.separator();
            ui.label(RichText::new(format!("{} fixed", fmt_count(fixed as u64))).color(if fixed > 0 { p.ok } else { p.text }));
        });
        ui.separator();
        if cmp.problems.is_empty() {
            ui.label(RichText::new("No problem changed between the two crawls.").color(p.muted));
            return;
        }
        let rows: Vec<Vec<Cell>> = cmp
            .problems
            .iter()
            .map(|d| {
                let (new, fixed) = (d.new.len() as u64, d.fixed.len() as u64);
                vec![
                    Cell::text(model::problem_name(d.view, d.filter)),
                    Cell::num(d.before as f64, fmt_count(d.before)),
                    Cell::num(d.now as f64, fmt_count(d.now)),
                    Cell::num(new as f64, fmt_count(new)).color(if new > 0 { p.danger } else { p.muted }),
                    Cell::num(fixed as f64, fmt_count(fixed)).color(if fixed > 0 { p.ok } else { p.muted }),
                ]
            })
            .collect();
        let table_h = (ui.available_height() * 0.55).max(140.0);
        let cols = [
            GridCol::text("Issue Name", (ui.available_width() - 272.0).max(140.0)),
            GridCol::num("Previous", 70.0),
            GridCol::num("Current", 64.0),
            GridCol::num("Added", 58.0),
            GridCol::num("Removed", 70.0),
        ];
        let clicked = ui
            .allocate_ui(vec2(ui.available_width(), table_h), |ui| grid(ui, p, "crawl-compare", &cols, &rows, self.selected_change))
            .inner
            .clicked
            .map(|(i, _)| i);
        if clicked.is_some() {
            self.selected_change = clicked;
        }
        ui.separator();
        let mut select_url = None;
        if let Some(d) = self.selected_change.and_then(|i| self.comparison.as_ref()?.problems.get(i)) {
            egui::ScrollArea::vertical().id_salt("change-urls").auto_shrink([false, false]).show(ui, |ui| {
                for (label, urls, color) in [("Added", &d.new, p.danger), ("Removed", &d.fixed, p.ok)] {
                    if urls.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(format!("{label} ({})", urls.len())).family(theme::semibold()).color(color));
                    for u in urls.iter().take(500) {
                        if ui.add(egui::Label::new(RichText::new(u).color(p.accent)).sense(Sense::click()).truncate()).on_hover_text(u).clicked() {
                            select_url = Some(u.clone());
                        }
                    }
                    ui.add_space(4.0);
                }
            });
        } else {
            ui.label(RichText::new("Select a row to list the URLs added to and removed from it.").color(p.muted));
        }
        if let Some(u) = select_url {
            self.select(Some(u));
        }
    }
}

/// A row in a tree list: label on the left, count and share on the right.
#[allow(clippy::too_many_arguments)]
pub fn tree_row(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    count: Option<u64>,
    share: Option<String>,
    expanded: Option<bool>,
    selected: bool,
    strong: bool,
) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 25.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::TreeItem, true, selected, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 3.0, p.selection);
            if strong {
                let pill = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 7.0), vec2(3.0, 14.0));
                painter.rect_filled(pill, 1.5, p.accent);
            }
        } else if resp.hovered() {
            painter.rect_filled(rect, 3.0, p.hover);
        }
        if let Some(open) = expanded {
            let c = rect.left_center() + vec2(12.0, 0.0);
            let pts = if open {
                vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
            } else {
                vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
            };
            painter.add(egui::Shape::convex_polygon(pts, p.muted, Stroke::NONE));
        }
        let font = if strong { egui::FontId::new(14.0, theme::semibold()) } else { egui::FontId::proportional(13.5) };
        let n = count.unwrap_or(0);
        let color = if n == 0 && !strong { p.muted } else { p.text };
        let x = rect.left() + if expanded.is_some() { 22.0 } else { 30.0 };
        let right = match (&share, count) {
            (Some(s), Some(c)) => format!("{}   {s}", fmt_count(c)),
            (None, Some(c)) => fmt_count(c),
            _ => String::new(),
        };
        let right_galley = painter.layout_no_wrap(right, font.clone(), if n == 0 { p.muted } else { p.text });
        let label_room = (rect.right() - 8.0 - right_galley.size().x - 8.0) - x;
        let label_galley = painter.layout(label.to_string(), font, color, f32::INFINITY);
        let clip = painter.with_clip_rect(egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(x + label_room.max(0.0), rect.bottom())));
        let cut = label_galley.size().x > label_room;
        clip.galley(egui::pos2(x, rect.center().y - label_galley.size().y / 2.0), label_galley, color);
        painter.galley(egui::pos2(rect.right() - 8.0 - right_galley.size().x, rect.center().y - right_galley.size().y / 2.0), right_galley, p.text);
        if cut {
            resp = resp.on_hover_text(label);
        }
    }
    resp
}

/// A small square before a filter row that is a problem.
fn draw_marker(ui: &egui::Ui, resp: &egui::Response, color: egui::Color32) {
    let c = resp.rect.left_center() + vec2(18.0, 0.0);
    ui.painter().rect_filled(egui::Rect::from_center_size(c, vec2(7.0, 7.0)), 1.5, color);
}

/// Error count beside a view's name, in the error colour.
fn draw_badge(ui: &egui::Ui, resp: &egui::Response, p: &Palette, n: u64) {
    let text = format!("{} errors", fmt_count(n));
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text, font, p.danger);
    let pos = egui::pos2(resp.rect.right() - 70.0 - galley.size().x, resp.rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, p.danger);
}
