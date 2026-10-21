//! The main window: state, data flow and layout. The pieces it is drawn
//! from live in `menus`, `toolbar`, `main_view`, `side_panel` and `details`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use engine::config::CrawlConfig;

use crate::diagram::DiagramWindow;
use crate::dialogs::{ConfigDialog, ConfigPage, HelpWindows, ListDialog, PrefsDialog};
use crate::model::{Severity, VIEWS};
use crate::paths;
use crate::prefs::{Mode, Prefs};
use crate::query::{Counts, Detail, ExportJob, ExportKind, Reply, Request, Row, Sort, Stats, View, Worker};
use crate::runner::CrawlRun;
use crate::theme;

/// While a crawl runs: how often the table, the counts and the charts are
/// re-read. Counts and charts touch every row, so they refresh less often.
const ROWS_REFRESH: Duration = Duration::from_millis(1000);
const COUNTS_REFRESH: Duration = Duration::from_millis(3000);
const STATS_REFRESH: Duration = Duration::from_millis(5000);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailTab {
    UrlDetails,
    Inlinks,
    Outlinks,
    Images,
    Resources,
    Serp,
    Source,
    Headers,
    Duplicates,
}

impl DetailTab {
    pub const ALL: [DetailTab; 9] = [
        Self::UrlDetails,
        Self::Inlinks,
        Self::Outlinks,
        Self::Images,
        Self::Resources,
        Self::Serp,
        Self::Source,
        Self::Headers,
        Self::Duplicates,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::UrlDetails => "URL Details",
            Self::Inlinks => "Inlinks",
            Self::Outlinks => "Outlinks",
            Self::Images => "Image Details",
            Self::Resources => "Resources",
            Self::Serp => "SERP Snippet",
            Self::Source => "View Source",
            Self::Headers => "HTTP Headers",
            Self::Duplicates => "Duplicate Details",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InsightTab {
    Problems,
    Breakdown,
    Structure,
    Speed,
    Changes,
}

impl InsightTab {
    pub const ALL: [InsightTab; 5] = [Self::Problems, Self::Breakdown, Self::Structure, Self::Speed, Self::Changes];

    pub fn label(self) -> &'static str {
        match self {
            Self::Problems => "Issues",
            Self::Breakdown => "Overview",
            Self::Structure => "Site Structure",
            Self::Speed => "Response Times",
            Self::Changes => "Crawl Compare",
        }
    }
}

pub enum NoticeKind {
    Info,
    Error,
}

pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
}

pub struct App {
    pub cfg: CrawlConfig,
    pub prefs: Prefs,
    pub prefs_dirty: bool,
    pub config_dialog: ConfigDialog,
    pub prefs_dialog: PrefsDialog,
    pub list_dialog: ListDialog,
    pub help: HelpWindows,
    pub diagram: DiagramWindow,

    pub address: String,
    pub list_urls: Vec<String>,
    pub run: Option<CrawlRun>,
    /// Start was pressed during a crawl: ask, then start once it has stopped.
    restart: Restart,
    /// The crawl file being shown (running or opened from disk).
    pub db: Option<PathBuf>,

    pub tab: usize,
    pub filter: usize,
    pub search: String,
    pub sort: Option<Sort>,
    pub worker: Worker,
    pub rows: Vec<Row>,
    /// The tab `rows` were loaded for; its columns are what the cells hold.
    pub rows_tab: Option<usize>,
    pub rows_truncated: bool,
    pub counts: Option<Counts>,
    pub stats: Option<Stats>,
    sent_rows: Option<(View, Instant)>,
    sent_counts: Option<Instant>,
    sent_stats: Option<Instant>,
    /// How long the last answer of each kind took, and whether one is
    /// outstanding. Live refreshes wait for the previous one and back off
    /// on big crawls, so the query thread never falls behind.
    rows_load: Load,
    counts_load: Load,
    stats_load: Load,

    pub selected: Option<String>,
    pub scroll_to: Option<usize>,
    pub detail: Option<Detail>,
    pub detail_tab: DetailTab,
    pub insight_tab: InsightTab,
    /// Tabs expanded in the overview tree, and the one charted below it.
    pub nav_expanded: HashSet<usize>,
    pub selected_problem: Option<(usize, usize)>,
    /// Severity the Issues list is narrowed to, if any.
    pub issue_severity: Option<Severity>,
    /// Bumped by Reset Layout: new panel ids start at their default sizes.
    pub layout_gen: u32,
    pub palette: crate::palette::Palette,
    /// The earlier crawl the Changes tab compares against, and the result.
    pub baseline: Option<PathBuf>,
    pub comparison: Option<crate::query::Comparison>,
    pub comparing: bool,
    pub selected_change: Option<usize>,
    /// Earlier crawls of the same site, found when Changes is first shown.
    pub baseline_candidates: Option<Vec<PathBuf>>,
    pub notice: Option<Notice>,
    pub focus_search: bool,
    /// The results table's vertical offset and visible height, last frame.
    pub table_scroll: (f32, f32),
    /// Where the results table was drawn last frame, and whether the last
    /// click landed in it (Ctrl+C then copies the selected URL).
    pub table_rect: egui::Rect,
    table_active: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let prefs = Prefs::load();
        theme::apply_prefs(&cc.egui_ctx, &prefs);
        let (cfg, notice) = match load_settings() {
            Ok(cfg) => (cfg, None),
            Err(e) => (
                CrawlConfig::default(),
                Some(Notice { text: format!("Settings could not be read, using defaults: {e}"), kind: NoticeKind::Error }),
            ),
        };
        Self {
            cfg,
            prefs,
            prefs_dirty: false,
            config_dialog: ConfigDialog::default(),
            prefs_dialog: PrefsDialog::default(),
            list_dialog: ListDialog::default(),
            help: HelpWindows::default(),
            diagram: DiagramWindow::default(),
            address: String::new(),
            list_urls: vec![],
            run: None,
            restart: Restart::No,
            db: None,
            tab: 0,
            filter: 0,
            search: String::new(),
            sort: None,
            worker: Worker::spawn(cc.egui_ctx.clone()),
            rows: vec![],
            rows_tab: None,
            rows_truncated: false,
            counts: None,
            stats: None,
            sent_rows: None,
            sent_counts: None,
            sent_stats: None,
            rows_load: Load::default(),
            counts_load: Load::default(),
            stats_load: Load::default(),
            selected: None,
            scroll_to: None,
            detail: None,
            detail_tab: DetailTab::UrlDetails,
            insight_tab: InsightTab::Problems,
            nav_expanded: HashSet::from([0]),
            selected_problem: None,
            issue_severity: None,
            layout_gen: 0,
            palette: Default::default(),
            baseline: None,
            comparison: None,
            comparing: false,
            selected_change: None,
            baseline_candidates: None,
            notice,
            focus_search: false,
            table_scroll: (0.0, 0.0),
            table_rect: egui::Rect::NOTHING,
            table_active: false,
        }
    }

    pub fn running(&self) -> bool {
        self.run.as_ref().is_some_and(CrawlRun::is_running)
    }

    pub fn notify(&mut self, text: impl Into<String>, kind: NoticeKind) {
        self.notice = Some(Notice { text: text.into(), kind });
    }

    // ---- crawl control -------------------------------------------------

    pub fn start_crawl(&mut self, ctx: &egui::Context) {
        if self.running() {
            if self.restart == Restart::No {
                self.restart = Restart::Asking;
            }
            return;
        }
        let (seeds, cfg, host) = match self.prefs.mode {
            Mode::Spider => {
                let seed = match normalize_address(&self.address) {
                    Ok(u) => u,
                    Err(e) => return self.notify(e, NoticeKind::Error),
                };
                self.address = seed.to_string();
                let host = seed.host_str().unwrap_or("site").to_string();
                (vec![seed.to_string()], self.cfg.clone(), host)
            }
            Mode::List => {
                if self.list_urls.is_empty() {
                    self.list_dialog.open = true;
                    return self.notify("Upload the URLs to check first.", NoticeKind::Error);
                }
                // List mode checks exactly these URLs.
                let cfg = CrawlConfig { max_depth: 0, follow_sitemaps: false, check_resources: false, crawl_external: false, ..self.cfg.clone() };
                (self.list_urls.clone(), cfg, "list".to_string())
            }
        };
        if let Err(e) = cfg.validate() {
            self.config_dialog.open_at(ConfigPage::Crawl, &self.cfg);
            return self.notify(format!("Fix the configuration first: {e}"), NoticeKind::Error);
        }
        if cfg.render_js && engine::render::find_browser(&cfg).is_none() {
            self.config_dialog.open_at(ConfigPage::Rendering, &self.cfg);
            return self.notify("JavaScript rendering needs Microsoft Edge or Google Chrome. Install one or turn rendering off.", NoticeKind::Error);
        }
        let db = paths::new_crawl_dir(&host).join("crawl.db");
        self.run = Some(CrawlRun::start(cfg, seeds, db.clone(), ctx.clone()));
        self.show_db(db);
        self.notice = None;
    }

    pub fn pause_or_resume(&mut self) {
        if let Some(r) = &self.run {
            if r.is_paused() { r.resume() } else { r.pause() }
        }
    }

    pub fn stop_crawl(&mut self) {
        if let Some(r) = &self.run {
            r.stop();
        }
    }

    /// Close the current crawl (the file stays on disk).
    pub fn clear(&mut self) {
        if self.running() {
            return self.notify("Stop the crawl before clearing it.", NoticeKind::Error);
        }
        self.run = None;
        self.db = None;
        self.reset_data();
    }

    fn reset_data(&mut self) {
        self.rows.clear();
        self.rows_tab = None;
        self.counts = None;
        self.stats = None;
        self.sent_rows = None;
        self.sent_counts = None;
        self.sent_stats = None;
        self.rows_load = Load::default();
        self.counts_load = Load::default();
        self.stats_load = Load::default();
        self.selected = None;
        self.detail = None;
        self.diagram.clear();
    }

    pub fn show_db(&mut self, db: PathBuf) {
        self.prefs.add_recent(db.clone());
        self.prefs_dirty = true;
        self.db = Some(db);
        self.reset_data();
        self.baseline = None;
        self.comparison = None;
        self.selected_change = None;
        self.baseline_candidates = None;
        if self.diagram.open {
            self.request_diagram();
        }
    }

    pub fn open_crawl(&mut self, path: PathBuf) {
        if self.running() {
            return self.notify("Stop the current crawl before opening another.", NoticeKind::Error);
        }
        if !path.is_file() {
            self.prefs.recent.retain(|p| p != &path);
            self.prefs_dirty = true;
            return self.notify(format!("{} no longer exists.", path.display()), NoticeKind::Error);
        }
        self.run = None;
        self.show_db(path);
    }

    pub fn open_crawl_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Open Crawl")
            .add_filter("Crawl file", &["db"])
            .set_directory(paths::crawls_dir())
            .pick_file();
        if let Some(path) = picked {
            self.open_crawl(path);
        }
    }

    pub fn save_copy_dialog(&mut self) {
        let Some(db) = self.db.clone() else { return };
        let name = db
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| format!("{}.db", n.to_string_lossy()))
            .unwrap_or_else(|| "crawl.db".into());
        if let Some(target) = rfd::FileDialog::new().set_title("Save Crawl As").add_filter("Crawl file", &["db"]).set_file_name(name).save_file() {
            self.worker.send(Request::SaveCopy { db, target });
            self.notify("Saving…", NoticeKind::Info);
        }
    }

    /// Ask where to save, then run an export on the query thread.
    pub fn export(&mut self, kind: ExportKind, title: &str, default_name: &str) {
        let Some(db) = self.db.clone() else {
            return self.notify("Open or run a crawl first.", NoticeKind::Error);
        };
        let folder = matches!(kind, ExportKind::AllIssues | ExportKind::PageSource);
        let target = if folder {
            rfd::FileDialog::new().set_title(title).pick_folder()
        } else {
            let ext = default_name.rsplit('.').next().unwrap_or("csv");
            rfd::FileDialog::new()
                .set_title(title)
                .add_filter(ext.to_uppercase(), &[ext])
                .set_file_name(default_name)
                .save_file()
        };
        if let Some(target) = target {
            self.worker.send(Request::Export(ExportJob { db, kind, target }));
            self.notify("Exporting…", NoticeKind::Info);
        }
    }

    pub fn export_current_view(&mut self) {
        let Some(view) = self.current_view() else {
            return self.notify("Open or run a crawl first.", NoticeKind::Error);
        };
        let t = &VIEWS[self.tab];
        let name = format!("{}_{}.csv", crate::query::slug(t.title), crate::query::slug(t.filters[self.filter].label));
        self.export(ExportKind::View(view), "Export", &name);
    }

    pub fn request_diagram(&mut self) {
        if let Some(db) = self.db.clone() {
            self.diagram.loading = true;
            self.worker.send(Request::Diagram(db));
        }
    }

    /// Compare the current crawl with `baseline` (Insights > Changes).
    pub fn set_baseline(&mut self, baseline: std::path::PathBuf) {
        self.baseline = Some(baseline.clone());
        self.comparison = None;
        self.selected_change = None;
        if let Some(db) = self.db.clone() {
            self.comparing = true;
            self.worker.send(Request::Compare { db, baseline });
        }
    }

    pub fn choose_baseline_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Compare with an earlier crawl")
            .add_filter("Crawl file", &["db"])
            .set_directory(paths::crawls_dir())
            .pick_file();
        if let Some(b) = picked {
            self.prefs.show_insights = true;
            self.insight_tab = InsightTab::Changes;
            self.set_baseline(b);
        }
    }

    pub fn select(&mut self, url: Option<String>) {
        if url == self.selected {
            return;
        }
        self.selected = url.clone();
        self.detail = None;
        if let (Some(url), Some(db)) = (url, self.db.clone()) {
            self.worker.send(Request::Detail { db, url });
        }
    }

    /// Switch the table to a tab and filter (from the overview or issues).
    pub fn go_to(&mut self, tab: usize, filter: usize) {
        if self.tab != tab {
            self.sort = None;
            // The loaded rows hold the old tab's columns.
            self.rows.clear();
            self.rows_tab = None;
        }
        self.tab = tab;
        self.filter = filter;
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.cfg.save(&paths::settings_path()) {
            self.notify(format!("Settings could not be saved: {e}"), NoticeKind::Error);
        }
    }

    pub fn load_profile(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("Load crawl settings").add_filter("Crawl settings", &["toml"]).pick_file() {
            match CrawlConfig::load(&path).and_then(|c| c.validate().map(|_| c)) {
                Ok(c) => {
                    self.cfg = c;
                    self.save_settings();
                    self.notify(format!("Loaded configuration from {}", path.display()), NoticeKind::Info);
                }
                Err(e) => self.notify(format!("Could not load {}: {e}", path.display()), NoticeKind::Error),
            }
        }
    }

    pub fn save_profile(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save crawl settings as")
            .add_filter("Crawl settings", &["toml"])
            .set_file_name("shoutingmonkey-config.toml")
            .save_file()
        {
            match self.cfg.save(&path) {
                Ok(()) => self.notify(format!("Saved configuration to {}", path.display()), NoticeKind::Info),
                Err(e) => self.notify(format!("Could not save {}: {e}", path.display()), NoticeKind::Error),
            }
        }
    }

    pub fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        self.prefs.zoom = ((zoom * 20.0).round() / 20.0).clamp(0.75, 2.0);
        theme::apply_prefs(ctx, &self.prefs);
        self.prefs_dirty = true;
    }

    // ---- data flow -----------------------------------------------------

    pub fn current_view(&self) -> Option<View> {
        let db = self.db.clone()?;
        Some(View { db, tab: self.tab, filter: self.filter, search: self.search.clone(), sort: self.sort })
    }

    fn request_data(&mut self, ctx: &egui::Context) {
        let Some(view) = self.current_view() else { return };
        if !view.db.is_file() {
            ctx.request_repaint_after(Duration::from_millis(250));
            return; // the crawl has not created its file yet
        }
        let running = self.running();
        let rows_due = match &self.sent_rows {
            Some((last, at)) => *last != view || (running && self.rows_load.due(*at, ROWS_REFRESH)),
            None => true,
        };
        if rows_due {
            self.worker.send(Request::Rows(view.clone()));
            self.sent_rows = Some((view.clone(), Instant::now()));
            self.rows_load.pending = true;
        }
        if self.sent_counts.is_none_or(|at| running && self.counts_load.due(at, COUNTS_REFRESH)) {
            self.worker.send(Request::Counts(view.db.clone()));
            self.sent_counts = Some(Instant::now());
            self.counts_load.pending = true;
        }
        let wants_stats = self.prefs.show_insights && matches!(self.insight_tab, InsightTab::Structure | InsightTab::Speed);
        if wants_stats && self.sent_stats.is_none_or(|at| running && self.stats_load.due(at, STATS_REFRESH)) {
            self.worker.send(Request::Stats(view.db.clone()));
            self.sent_stats = Some(Instant::now());
            self.stats_load.pending = true;
        }
        if running {
            ctx.request_repaint_after(ROWS_REFRESH);
        }
    }

    fn receive(&mut self) {
        while let Some(reply) = self.worker.poll() {
            let shown = self.db.clone();
            let showing = |db: &PathBuf| shown.as_ref() == Some(db);
            match reply {
                Reply::Rows(Ok(r)) => {
                    self.rows_load.answered(self.sent_rows.as_ref().map(|(_, at)| *at));
                    if showing(&r.view.db) {
                        self.rows = r.rows;
                        self.rows_tab = Some(r.view.tab);
                        self.rows_truncated = r.truncated;
                    }
                }
                Reply::Counts(Ok(c)) => {
                    self.counts_load.answered(self.sent_counts);
                    if showing(&c.db) {
                        self.counts = Some(c);
                    }
                }
                Reply::Stats(Ok(s)) => {
                    self.stats_load.answered(self.sent_stats);
                    if showing(&s.db) {
                        self.stats = Some(s);
                    }
                }
                Reply::Diagram(Ok(d)) => {
                    if showing(&d.db) {
                        self.diagram.set_data(d);
                    }
                }
                Reply::Compare(Ok(c)) => {
                    if showing(&c.db) && self.baseline.as_ref() == Some(&c.baseline) {
                        self.comparison = Some(*c);
                        self.comparing = false;
                    }
                }
                Reply::Compare(Err(e)) => {
                    self.comparing = false;
                    self.notify(format!("Could not compare the crawls: {e}"), NoticeKind::Error);
                }
                Reply::Detail(Ok(d)) => {
                    if self.selected.as_deref() == Some(d.url.as_str()) {
                        self.detail = Some(*d);
                    }
                }
                Reply::Rows(Err(e)) | Reply::Counts(Err(e)) | Reply::Stats(Err(e)) | Reply::Detail(Err(e)) | Reply::Diagram(Err(e)) => {
                    // Let the next refresh go out (the file may simply not be ready).
                    self.rows_load.pending = false;
                    self.counts_load.pending = false;
                    self.stats_load.pending = false;
                    if !self.running() {
                        self.notify(format!("Could not read the crawl: {e}"), NoticeKind::Error);
                    }
                }
                Reply::Done(Ok(msg)) => self.notify(msg, NoticeKind::Info),
                Reply::Done(Err(e)) => self.notify(format!("Failed: {e}"), NoticeKind::Error),
            }
        }
    }

    /// Confirm replacing a running crawl, and start the new one once the
    /// old one has stopped (its file is kept).
    fn restart_prompt(&mut self, ctx: &egui::Context) {
        match self.restart {
            Restart::No => {}
            Restart::Asking => {
                let mut choice = None;
                let modal = egui::Modal::new(egui::Id::unique("restart-crawl")).show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.heading("Start a new crawl?");
                    ui.add_space(4.0);
                    ui.label("A crawl is still running. It will be stopped and saved, and the new one started.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Stop and start new").clicked() {
                            choice = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(false);
                        }
                    });
                });
                if modal.should_close() && choice.is_none() {
                    choice = Some(false);
                }
                match choice {
                    Some(true) => {
                        self.stop_crawl();
                        self.restart = Restart::AfterStop;
                    }
                    Some(false) => self.restart = Restart::No,
                    None => {}
                }
            }
            Restart::AfterStop if self.running() => ctx.request_repaint_after(Duration::from_millis(100)),
            Restart::AfterStop => {
                self.restart = Restart::No;
                self.start_crawl(ctx);
            }
        }
    }

    /// A last refresh when a crawl ends, plus a status message.
    fn watch_run_end(&mut self) {
        let Some(run) = &mut self.run else { return };
        let s = run.snapshot();
        let Some(outcome) = &s.outcome else { return };
        if std::mem::replace(&mut run.end_reported, true) {
            return;
        }
        let elapsed = fmt_duration(run.elapsed());
        self.notice = Some(match outcome {
            Ok(sum) if sum.cancelled => Notice { text: format!("Crawl stopped after {} URLs ({elapsed}).", fmt_count(sum.crawled)), kind: NoticeKind::Info },
            Ok(sum) => Notice { text: format!("Crawl finished: {} URLs in {elapsed}.", fmt_count(sum.crawled)), kind: NoticeKind::Info },
            Err(e) => Notice { text: format!("Crawl failed: {e}"), kind: NoticeKind::Error },
        });
        self.sent_rows = None;
        self.sent_counts = None;
        self.sent_stats = None;
        self.rows_load = Load::default();
        self.counts_load = Load::default();
        self.stats_load = Load::default();
        if self.diagram.open {
            self.request_diagram();
        }
        if let Some(b) = self.baseline.clone() {
            self.set_baseline(b);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let shortcut = |key| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key));
        if shortcut(Key::O) {
            self.open_crawl_dialog();
        }
        if shortcut(Key::S) {
            self.save_copy_dialog();
        }
        if shortcut(Key::W) {
            self.clear();
        }
        if shortcut(Key::F) {
            self.focus_search = true;
        }
        if shortcut(Key::K) {
            self.palette.open();
        }
        if self.palette.is_open() {
            return; // the palette owns the arrow keys
        }
        if shortcut(Key::Enter) {
            self.start_crawl(ctx);
        }
        if shortcut(Key::Plus) || shortcut(Key::Equals) {
            self.set_zoom(ctx, self.prefs.zoom + 0.1);
        }
        if shortcut(Key::Minus) {
            self.set_zoom(ctx, self.prefs.zoom - 0.1);
        }
        if shortcut(Key::Num0) {
            self.set_zoom(ctx, 1.0);
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if let Some(pos) = ctx.input(|i| i.pointer.any_click().then(|| i.pointer.interact_pos()).flatten()) {
            self.table_active = self.table_rect.contains(pos);
        }
        if let Some(url) = self.selected.clone().filter(|_| self.table_active)
            && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::C))
        {
            ctx.copy_text(url);
        }
        if self.rows.is_empty() {
            return;
        }
        let (down, up, esc) = ctx.input(|i| (i.key_pressed(Key::ArrowDown), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::Escape)));
        if esc {
            self.select(None);
            return;
        }
        if !(down || up) {
            return;
        }
        let current = self.selected.as_ref().and_then(|s| self.rows.iter().position(|r| &r.url == s));
        let next = match (current, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(self.rows.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.scroll_to = Some(next);
        self.select(Some(self.rows[next].url.clone()));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive();
        self.watch_run_end();
        self.handle_keys(&ctx);
        self.request_data(&ctx);

        let p = theme::palette(ui);
        egui::Panel::top("menu")
            .frame(egui::Frame::new().fill(p.header).inner_margin(egui::Margin::symmetric(6, 3)))
            .show(ui, |ui| {
                theme::header_visuals(ui, p);
                self.menu_bar(ui);
            });
        egui::Panel::top("toolbar")
            .frame(egui::Frame::new().fill(p.toolbar).inner_margin(egui::Margin::symmetric(8, 7)).stroke(egui::Stroke::new(1.0, p.border)))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(p.status).inner_margin(egui::Margin::symmetric(10, 4)).stroke(egui::Stroke::new(1.0, p.border)))
            .show(ui, |ui| self.status_bar(ui));
        if self.prefs.show_navigator {
            egui::Panel::left(("navigator", self.layout_gen))
                .resizable(true)
                .default_size(280.0)
                .min_size(220.0)
                .frame(egui::Frame::new().fill(p.window).inner_margin(egui::Margin::symmetric(6, 4)))
                .show(ui, |ui| self.navigator(ui));
        }
        if self.prefs.show_insights {
            egui::Panel::right(("insights", self.layout_gen))
                .resizable(true)
                .default_size(450.0)
                .min_size(300.0)
                .frame(egui::Frame::new().fill(p.window).inner_margin(egui::Margin::symmetric(8, 6)))
                .show(ui, |ui| self.insights_panel(ui));
        }
        if self.prefs.show_inspector {
            egui::Panel::bottom(("details", self.layout_gen))
                .resizable(true)
                .default_size(250.0)
                .min_size(120.0)
                .frame(egui::Frame::new().fill(p.window).inner_margin(egui::Margin::symmetric(8, 4)))
                .show(ui, |ui| self.details_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.window).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| self.main_view(ui));

        // Dialogs and other windows.
        let running = self.running();
        if let Some(cfg) = self.config_dialog.show(&ctx, running) {
            self.cfg = cfg;
            self.save_settings();
        }
        if self.prefs_dialog.show(&ctx, &mut self.prefs) {
            theme::apply_prefs(&ctx, &self.prefs);
            self.prefs_dirty = true;
        }
        if let Some(urls) = self.list_dialog.show(&ctx) {
            self.list_urls = urls;
            self.start_crawl(&ctx);
        }
        self.restart_prompt(&ctx);
        self.help.show(&ctx);
        self.show_palette(&ctx);
        if let Some(url) = self.diagram.show(&ctx) {
            self.select(Some(url));
        }

        if self.prefs_dirty {
            if let Err(e) = self.prefs.save() {
                tracing::warn!("could not save preferences: {e:#}");
            }
            self.prefs_dirty = false;
        }
    }

    fn on_exit(&mut self) {
        let _ = self.prefs.save();
        // Let a running crawl stop cleanly so its file is marked "stopped".
        if let Some(run) = &self.run {
            run.stop();
            let deadline = Instant::now() + Duration::from_secs(5);
            while run.is_running() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Restart {
    No,
    Asking,
    AfterStop,
}

/// Timing of one kind of query, for pacing live refreshes.
#[derive(Default)]
struct Load {
    last: Duration,
    pending: bool,
}

impl Load {
    /// A refresh is due once the previous answer is in and `base` (or
    /// three times the last query's duration, if longer) has passed.
    fn due(&self, sent: Instant, base: Duration) -> bool {
        !self.pending && sent.elapsed() >= base.max(self.last * 3)
    }

    fn answered(&mut self, sent: Option<Instant>) {
        self.pending = false;
        if let Some(at) = sent {
            self.last = at.elapsed();
        }
    }
}

fn load_settings() -> anyhow::Result<CrawlConfig> {
    let path = paths::settings_path();
    if path.is_file() { CrawlConfig::load(&path) } else { Ok(CrawlConfig::default()) }
}

/// Accept "example.com" as well as full URLs; only http(s) is crawlable.
pub fn normalize_address(input: &str) -> Result<url::Url, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("Enter a website address to crawl, for example https://example.com".into());
    }
    let with_scheme = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    let u = url::Url::parse(&with_scheme).map_err(|_| format!("\"{s}\" is not a valid web address."))?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none_or(|h| !h.contains('.') && h != "localhost") {
        return Err(format!("\"{s}\" is not a website address. Use one like https://example.com"));
    }
    Ok(u)
}

/// Header click cycle: first click sorts (numbers largest first), the
/// second reverses, the third returns to crawl order.
pub fn next_sort(current: Option<Sort>, column: &'static str, numeric: bool) -> Option<Sort> {
    let first = Sort { column, descending: numeric };
    match current {
        Some(s) if s.column == column && s.descending == first.descending => Some(Sort { column, descending: !s.descending }),
        Some(s) if s.column == column => None,
        _ => Some(first),
    }
}

pub fn fmt_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        _ => format!("{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_normalization() {
        assert_eq!(normalize_address(" example.com ").unwrap().as_str(), "https://example.com/");
        assert_eq!(normalize_address("http://a.b/x").unwrap().as_str(), "http://a.b/x");
        assert!(normalize_address("").is_err());
        assert!(normalize_address("ftp://a.b").is_err());
        assert!(normalize_address("hello").is_err());
        assert!(normalize_address("localhost:8080").is_ok());
    }

    #[test]
    fn sort_cycle() {
        let a = next_sort(None, "size", true);
        assert_eq!(a, Some(Sort { column: "size", descending: true }));
        let b = next_sort(a, "size", true);
        assert_eq!(b, Some(Sort { column: "size", descending: false }));
        assert_eq!(next_sort(b, "size", true), None);
        assert_eq!(next_sort(b, "address", false), Some(Sort { column: "address", descending: false }));
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(1234567), "1,234,567");
        assert_eq!(fmt_duration(Duration::from_secs(134)), "2 min 14 s");
    }
}
