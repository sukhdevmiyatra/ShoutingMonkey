//! The toolbar (address, crawl buttons, progress) and the status bar.

use egui::{Align, Key, Layout, RichText, Stroke, vec2};
use engine::crawl::Phase;

use crate::app::{App, NoticeKind, fmt_count};
use crate::prefs::Mode;
use crate::theme;

const BUTTON: egui::Vec2 = vec2(78.0, 28.0);

impl App {
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let running = self.running();
        let paused = self.run.as_ref().is_some_and(|r| r.is_paused());
        let stopping = self.run.as_ref().is_some_and(|r| r.stopping());
        ui.horizontal(|ui| {
            // Four buttons, the progress meter and spacing.
            let right_w = 4.0 * (BUTTON.x + 8.0) + 200.0 + 24.0;
            match self.prefs.mode {
                Mode::Spider => {
                    let edit = egui::TextEdit::singleline(&mut self.address)
                        .hint_text("Enter URL to crawl, for example example.com")
                        .desired_width((ui.available_width() - right_w).max(220.0))
                        .margin(vec2(8.0, 5.0));
                    // Editable during a crawl: Start then offers to replace it.
                    let resp = ui.add(edit);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.start_crawl(&ctx);
                    }
                }
                Mode::List => {
                    if ui.add_enabled(!running, egui::Button::new("Upload…").min_size(BUTTON)).clicked() {
                        self.list_dialog.open = true;
                    }
                    let text = if self.list_urls.is_empty() {
                        "List mode: upload the URLs to check.".to_string()
                    } else {
                        format!("List mode: {} URLs ready.", fmt_count(self.list_urls.len() as u64))
                    };
                    ui.add_sized([(ui.available_width() - right_w).max(200.0), 28.0], egui::Label::new(RichText::new(text).color(p.muted)));
                }
            }

            let start = egui::Button::new(RichText::new("Start").color(p.on_accent)).fill(p.accent).stroke(Stroke::NONE).min_size(BUTTON);
            let hint = if running { "Stop this crawl and start a new one (Ctrl+Enter)" } else { "Start crawling (Ctrl+Enter)" };
            if ui.add_enabled(!stopping, start).on_hover_text(hint).clicked() {
                self.start_crawl(&ctx);
            }
            let pause_label = if paused { "Resume" } else { "Pause" };
            if ui.add_enabled(running && !stopping, egui::Button::new(pause_label).min_size(BUTTON)).clicked() {
                self.pause_or_resume();
            }
            if ui.add_enabled(running && !stopping, egui::Button::new(if stopping { "Stopping…" } else { "Stop" }).min_size(BUTTON)).clicked() {
                self.stop_crawl();
            }
            if ui.add_enabled(!running && self.db.is_some(), egui::Button::new("Clear").min_size(BUTTON)).on_hover_text("Close this crawl (it stays saved)").clicked() {
                self.clear();
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (fraction, label) = match &mut self.run {
                    Some(run) => {
                        let s = run.snapshot();
                        let f = run.fraction(&s);
                        let label = match (&s.outcome, s.phase) {
                            (Some(Ok(sum)), _) if sum.cancelled => "Stopped".to_string(),
                            (Some(Ok(_)), _) => "Completed".to_string(),
                            (Some(Err(_)), _) => "Failed".to_string(),
                            (None, Phase::Discovering) => "Reading robots.txt".to_string(),
                            (None, Phase::Paused) => format!("Paused at {:.0}%", f * 100.0),
                            (None, Phase::SitemapUrls) => format!("Sitemap URLs {:.0}%", f * 100.0),
                            (None, Phase::Analysing | Phase::Finished) => "Analysing".to_string(),
                            (None, Phase::Crawling) => format!("{:.0}%", f * 100.0),
                        };
                        (f, label)
                    }
                    None => (0.0, "Not started".to_string()),
                };
                // Text beside the bar, not on it: legible on any fill.
                let bar = egui::ProgressBar::new(fraction).desired_width(110.0).desired_height(8.0);
                ui.add(if fraction > 0.0 { bar.fill(p.accent) } else { bar });
                ui.label(label);
            });
        });
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let mode = match self.prefs.mode {
            Mode::Spider => "Site crawl",
            Mode::List => "URL list",
        };
        let (state, rates, completed) = match &mut self.run {
            Some(run) => {
                let s = run.snapshot();
                let state = match (&s.outcome, s.phase) {
                    (Some(_), _) => "Ready",
                    (None, _) if run.stopping() => "Stopping",
                    (None, Phase::Paused) => "Paused",
                    (None, Phase::Discovering) => "Reading robots.txt and sitemaps",
                    (None, Phase::SitemapUrls) => "Crawling URLs only the sitemaps list",
                    (None, Phase::Analysing | Phase::Finished) => "Working out near duplicates and link scores",
                    (None, Phase::Crawling) => "Crawling",
                };
                let average = run.average_rate(s.crawled);
                let rates = if s.outcome.is_some() {
                    format!("{average:.1} pages a second on average")
                } else {
                    format!("{:.1} pages a second now, {average:.1} on average", run.current_rate())
                };
                let (done, total) = run.progress(&s);
                let pct = run.fraction(&s) * 100.0;
                let remaining = s.queued;
                let progress = match &s.outcome {
                    None => format!(
                        "{} of {} found so far ({pct:.0}%), {} in progress, {} waiting",
                        fmt_count(done),
                        fmt_count(total),
                        fmt_count(s.in_flight),
                        fmt_count(remaining)
                    ),
                    Some(Ok(sum)) if remaining > 0 => format!(
                        "{} crawled, {} not visited ({})",
                        fmt_count(done),
                        fmt_count(remaining),
                        if sum.cancelled { "stopped" } else { "URL limit reached" }
                    ),
                    Some(_) => format!("{} crawled", fmt_count(done)),
                };
                (state, rates, progress)
            }
            None => ("Ready", String::new(), String::new()),
        };
        ui.horizontal(|ui| {
            ui.label(format!("{mode}: {state}"));
            ui.separator();
            ui.label(RichText::new(rates).color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(completed);
                ui.separator();
                if let Some(n) = &self.notice {
                    let color = match n.kind {
                        NoticeKind::Info => p.text,
                        NoticeKind::Error => p.danger,
                    };
                    let text = n.text.clone();
                    if ui.small_button("Dismiss").clicked() {
                        self.notice = None;
                    }
                    ui.add(egui::Label::new(RichText::new(text).color(color)).truncate());
                } else if let Some(db) = &self.db {
                    let path = db.display().to_string();
                    if ui.add(egui::Label::new(RichText::new(&path).small().color(p.muted)).truncate().sense(egui::Sense::click()))
                        .on_hover_text("Click to show in Explorer")
                        .clicked()
                    {
                        crate::paths::reveal_in_explorer(db);
                    }
                }
            });
        });
    }
}
