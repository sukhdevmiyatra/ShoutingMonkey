//! Dialog windows: crawl configuration, preferences, list mode, about,
//! keyboard shortcuts and the user guide.

use egui::{Align, Align2, Layout, RichText};
use engine::config::{CrawlConfig, RobotsMode};

use crate::prefs::{Prefs, ThemeChoice};
use crate::theme::{self, Palette};
use crate::paths;

/// A centred, fixed dialog window.
fn dialog(title: &str, ctx: &egui::Context, size: [f32; 2]) -> egui::Window<'static> {
    egui::Window::new(title.to_string())
        .collapsible(false)
        .resizable(false)
        .fixed_size(size)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).family(theme::semibold()).size(15.0));
    ui.add_space(2.0);
}

fn hint(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).color(p.muted).small());
}

fn grid_row(ui: &mut egui::Ui, label: &str, widget: impl egui::Widget) -> bool {
    ui.label(label);
    let changed = ui.add(widget).changed();
    ui.end_row();
    changed
}

/// Bottom-right dialog buttons; returns which was pressed.
#[derive(PartialEq, Eq)]
enum Button {
    Ok,
    Cancel,
    Apply,
}

fn ok_cancel_apply(ui: &mut egui::Ui, ok_enabled: bool, with_apply: bool) -> Option<Button> {
    let mut pressed = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let size = egui::vec2(80.0, 26.0);
        if with_apply && ui.add_enabled(ok_enabled, egui::Button::new("Apply").min_size(size)).clicked() {
            pressed = Some(Button::Apply);
        }
        if ui.add(egui::Button::new("Cancel").min_size(size)).clicked() {
            pressed = Some(Button::Cancel);
        }
        if ui.add_enabled(ok_enabled, egui::Button::new("OK").min_size(size)).clicked() {
            pressed = Some(Button::Ok);
        }
    });
    pressed
}

// ---- configuration --------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPage {
    Crawl,
    Limits,
    Rendering,
    Advanced,
    Robots,
    Include,
    Exclude,
    Speed,
    UserAgent,
}

impl ConfigPage {
    pub const ALL: [ConfigPage; 9] = [
        Self::Crawl,
        Self::Limits,
        Self::Speed,
        Self::Rendering,
        Self::Robots,
        Self::Include,
        Self::Exclude,
        Self::UserAgent,
        Self::Advanced,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crawl => "Spider",
            Self::Limits => "Limits",
            Self::Rendering => "Rendering",
            Self::Advanced => "Advanced",
            Self::Robots => "robots.txt",
            Self::Include => "Include",
            Self::Exclude => "Exclude",
            Self::Speed => "Speed",
            Self::UserAgent => "User-Agent",
        }
    }
}

const USER_AGENTS: &[(&str, &str)] = &[
    ("ShoutingMonkey (default)", ""),
    ("Googlebot Smartphone", "Mozilla/5.0 (Linux; Android 6.0.1; Nexus 5X Build/MMB29P) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"),
    ("Googlebot Desktop", "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"),
    ("Bingbot", "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)"),
    ("Chrome on Windows", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"),
    ("Safari on iPhone", "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1"),
];

pub struct ConfigDialog {
    pub open: bool,
    page: ConfigPage,
    draft: CrawlConfig,
    include_text: String,
    exclude_text: String,
    test_url: String,
    detected_browser: Option<Option<String>>,
}

impl Default for ConfigDialog {
    fn default() -> Self {
        Self {
            open: false,
            page: ConfigPage::Crawl,
            draft: CrawlConfig::default(),
            include_text: String::new(),
            exclude_text: String::new(),
            test_url: String::new(),
            detected_browser: None,
        }
    }
}

impl ConfigDialog {
    pub fn open_at(&mut self, page: ConfigPage, cfg: &CrawlConfig) {
        self.open = true;
        self.page = page;
        self.draft = cfg.clone();
        self.include_text = cfg.include_patterns.join("\n");
        self.exclude_text = cfg.exclude_patterns.join("\n");
        self.detected_browser = None;
    }

    /// Shows the dialog; returns a configuration to apply on OK or Apply.
    pub fn show(&mut self, ctx: &egui::Context, crawl_running: bool) -> Option<CrawlConfig> {
        if !self.open {
            return None;
        }
        let mut apply = None;
        let mut open = true;
        dialog("Configuration", ctx, [800.0, 580.0]).open(&mut open).show(ctx, |ui| {
            let p = theme::palette(ui);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    for page in ConfigPage::ALL {
                        let selected = self.page == page;
                        if ui.add_sized([160.0, 26.0], egui::Button::selectable(selected, page.label())).clicked() {
                            self.page = page;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(580.0);
                    ui.set_min_height(480.0);
                    ui.heading(self.page.label());
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(470.0).auto_shrink([false, false]).show(ui, |ui| self.page_ui(ui, p));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Restore Defaults").on_hover_text("Reset every setting on every page").clicked() {
                    self.draft = CrawlConfig::default();
                    self.include_text.clear();
                    self.exclude_text.clear();
                }
                let valid = self.draft.validate();
                match &valid {
                    Err(e) => {
                        ui.colored_label(p.danger, e.to_string());
                    }
                    Ok(()) if crawl_running => {
                        ui.colored_label(p.muted, "Changes apply to the next crawl.");
                    }
                    Ok(()) => {}
                }
                match ok_cancel_apply(ui, valid.is_ok(), true) {
                    Some(Button::Ok) => {
                        apply = Some(self.draft.clone());
                        self.open = false;
                    }
                    Some(Button::Apply) => apply = Some(self.draft.clone()),
                    Some(Button::Cancel) => self.open = false,
                    None => {}
                }
            });
        });
        if !open {
            self.open = false;
        }
        apply
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let cfg = &mut self.draft;
        match self.page {
            ConfigPage::Crawl => {
                section(ui, "Resource links");
                ui.checkbox(&mut cfg.check_resources, "Check images, CSS, JavaScript and iframes");
                ui.checkbox(&mut cfg.crawl_external, "Check external links (status only)");
                section(ui, "Page links");
                ui.checkbox(&mut cfg.include_subdomains, "Crawl all subdomains");
                hint(ui, p, "On: every host under the site's domain, so starting at blog.example.com also crawls www.example.com and shop.example.com. The www and bare addresses always count as one site.");
                ui.checkbox(&mut cfg.crawl_outside_start_folder, "Crawl outside of start folder");
                hint(ui, p, "Off: starting at example.com/blog/ crawls only /blog/; links elsewhere are checked but not followed.");
                ui.checkbox(&mut cfg.follow_internal_nofollow, "Follow internal \"nofollow\" links");
                ui.checkbox(&mut cfg.follow_canonicals, "Crawl canonicals");
                ui.checkbox(&mut cfg.follow_hreflang, "Crawl hreflang");
                let mut unrestricted = !cfg.restrict_to_seed_domain;
                if ui.checkbox(&mut unrestricted, "Keep following links onto other sites").changed() {
                    cfg.restrict_to_seed_domain = !unrestricted;
                }
                if unrestricted {
                    hint(ui, p, "The crawl will wander across the web until it reaches the URL limit.");
                }
                section(ui, "Discovery");
                ui.checkbox(&mut cfg.follow_sitemaps, "Crawl linked XML sitemaps (from robots.txt, or /sitemap.xml)");
                hint(ui, p, "Sitemap URLs no link leads to are crawled after the link crawl and shown under Sitemaps > Orphan URLs. Sites without robots.txt or a sitemap crawl normally.");
            }
            ConfigPage::Limits => {
                egui::Grid::new("limits").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    grid_row(ui, "Limit crawl total", egui::DragValue::new(&mut cfg.max_urls).range(1..=50_000_000).suffix(" URLs").speed(10));
                    grid_row(ui, "Limit crawl depth", egui::DragValue::new(&mut cfg.max_depth).range(0..=1000).suffix(" clicks"));
                    grid_row(ui, "Max redirects to follow", egui::DragValue::new(&mut cfg.max_redirects).range(1..=50));
                    let mut mb = (cfg.max_response_bytes / (1024 * 1024)).max(1);
                    if grid_row(ui, "Max page size to read", egui::DragValue::new(&mut mb).range(1..=500).suffix(" MB")) {
                        cfg.max_response_bytes = mb * 1024 * 1024;
                    }
                });
                hint(ui, p, "Depth counts clicks from the start URL; 0 crawls only the start URL.");
            }
            ConfigPage::Rendering => {
                ui.checkbox(&mut cfg.render_js, "Render pages with JavaScript before reading them");
                hint(ui, p, "For sites built with React, Vue, Angular and similar, where content and links only appear after scripts run. Rendering is several times slower than reading the HTML.");
                ui.add_space(8.0);
                ui.add_enabled_ui(cfg.render_js, |ui| {
                    egui::Grid::new("render").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        grid_row(ui, "Wait after page load", egui::DragValue::new(&mut cfg.render_wait_ms).range(0..=30_000).speed(50).suffix(" ms"));
                        ui.label("Browser");
                        let detected = self.detected_browser.get_or_insert_with(|| {
                            let probe = CrawlConfig { browser_path: String::new(), ..cfg.clone() };
                            engine::render::find_browser(&probe).map(|p| p.display().to_string())
                        });
                        let placeholder = detected.as_deref().unwrap_or("No Edge or Chrome found: enter a path");
                        ui.add(egui::TextEdit::singleline(&mut cfg.browser_path).hint_text(placeholder).desired_width(360.0));
                        ui.end_row();
                    });
                    if cfg.browser_path.trim().is_empty() {
                        match self.detected_browser.as_ref().and_then(|d| d.as_deref()) {
                            Some(found) => hint(ui, p, &format!("Using {found}")),
                            None => {
                                ui.colored_label(p.danger, "Install Microsoft Edge or Google Chrome, or enter the path to one.");
                            }
                        }
                    }
                });
            }
            ConfigPage::Advanced => {
                egui::Grid::new("advanced").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    grid_row(ui, "Response timeout", egui::DragValue::new(&mut cfg.timeout_secs).range(1..=600).suffix(" s"));
                    grid_row(ui, "Retries on failure", egui::DragValue::new(&mut cfg.retries).range(0..=10));
                });
                ui.add_space(6.0);
                ui.checkbox(&mut cfg.store_html, "Keep each page's HTML (for View Source and Bulk Export > Pages)");
            }
            ConfigPage::Robots => {
                for m in RobotsMode::ALL {
                    ui.radio_value(&mut cfg.robots_mode, m, m.label());
                }
                ui.add_space(6.0);
                hint(ui, p, match cfg.robots_mode {
                    RobotsMode::Respect => "Disallowed URLs are not fetched and are listed under Response Codes > Blocked by Robots.txt. Crawl-delay is honoured.",
                    RobotsMode::IgnoreButReport => "Every URL is fetched, and those robots.txt disallows are still flagged as blocked.",
                    RobotsMode::Ignore => "robots.txt is not read for crawling decisions at all.",
                });
            }
            ConfigPage::Include | ConfigPage::Exclude => {
                let including = self.page == ConfigPage::Include;
                hint(ui, p, if including {
                    "Only URLs matching at least one of these regular expressions are crawled. One per line. The start URL is always crawled."
                } else {
                    "URLs matching any of these regular expressions are not crawled. One per line. The start URL is always crawled."
                });
                let text = if including { &mut self.include_text } else { &mut self.exclude_text };
                let edit = egui::TextEdit::multiline(text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(9)
                    .desired_width(f32::INFINITY)
                    .hint_text(if including { "https://example.com/blog/.*" } else { ".*\\?sort=.*" });
                if ui.add(edit).changed() {
                    let lines: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
                    if including { cfg.include_patterns = lines } else { cfg.exclude_patterns = lines }
                }
                section(ui, "Test a URL");
                ui.add(egui::TextEdit::singleline(&mut self.test_url).hint_text("https://example.com/page").desired_width(f32::INFINITY));
                if !self.test_url.trim().is_empty() {
                    match cfg.compile_patterns() {
                        Err(e) => {
                            ui.colored_label(p.danger, e.to_string());
                        }
                        Ok((inc, exc)) => {
                            let u = self.test_url.trim();
                            let (text, color) = if !inc.is_empty() && !inc.iter().any(|r| r.is_match(u)) {
                                ("Not crawled: matches no include pattern.".to_string(), p.danger)
                            } else if let Some(r) = exc.iter().find(|r| r.is_match(u)) {
                                (format!("Not crawled: matches exclude pattern {}", r.as_str()), p.danger)
                            } else {
                                ("Crawled.".to_string(), p.ok)
                            };
                            ui.colored_label(color, text);
                        }
                    }
                }
            }
            ConfigPage::Speed => {
                egui::Grid::new("speed").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    grid_row(ui, "Max threads", egui::DragValue::new(&mut cfg.concurrency).range(1..=256));
                    grid_row(ui, "Limit URLs/s per site", egui::DragValue::new(&mut cfg.rps_per_host).range(0.0..=1000.0).speed(0.1).max_decimals(1));
                });
                hint(ui, p, "The default is 5 threads with no rate limit, like most desktop crawlers. 0 URLs/s means no limit; set one for sites that struggle under load.");
            }
            ConfigPage::UserAgent => {
                let current = USER_AGENTS
                    .iter()
                    .find(|(_, ua)| *ua == cfg.user_agent || (ua.is_empty() && cfg.user_agent == CrawlConfig::default().user_agent))
                    .map(|(n, _)| *n)
                    .unwrap_or("Custom");
                ui.horizontal(|ui| {
                    let caption = ui.label("Preset");
                    egui::ComboBox::from_id_salt("ua")
                        .width(280.0)
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (name, ua) in USER_AGENTS {
                                if ui.selectable_label(current == *name, *name).clicked() {
                                    cfg.user_agent = if ua.is_empty() { CrawlConfig::default().user_agent } else { ua.to_string() };
                                }
                            }
                        })
                        .response
                        .labelled_by(caption.id);
                });
                ui.add_space(6.0);
                ui.label("User-agent string");
                ui.add(egui::TextEdit::multiline(&mut cfg.user_agent).desired_rows(3).desired_width(f32::INFINITY));
                hint(ui, p, "robots.txt rules are matched against this string. Rendering uses it too.");
            }
        }
    }
}

// ---- preferences ------------------------------------------------------------

#[derive(Default)]
pub struct PrefsDialog {
    pub open: bool,
}

impl PrefsDialog {
    /// Returns true when a preference changed (it applies immediately).
    pub fn show(&mut self, ctx: &egui::Context, prefs: &mut Prefs) -> bool {
        if !self.open {
            return false;
        }
        let mut changed = false;
        let mut open = true;
        dialog("Settings", ctx, [480.0, 340.0]).open(&mut open).show(ctx, |ui| {
            let p = theme::palette(ui);
            section(ui, "Appearance");
            ui.horizontal(|ui| {
                ui.label("Theme");
                for t in ThemeChoice::ALL {
                    changed |= ui.radio_value(&mut prefs.theme, t, t.label()).changed();
                }
            });
            ui.horizontal(|ui| {
                let caption = ui.label("Text and interface size");
                let pct = (prefs.zoom * 100.0).round() as i32;
                egui::ComboBox::from_id_salt("zoom")
                    .selected_text(format!("{pct}%"))
                    .show_ui(ui, |ui| {
                        for z in [75, 90, 100, 110, 125, 150, 175, 200] {
                            let label = if z == 100 { "100% (default)".to_string() } else { format!("{z}%") };
                            if ui.selectable_label(pct == z, label).clicked() {
                                prefs.zoom = z as f32 / 100.0;
                                changed = true;
                            }
                        }
                    })
                    .response
                    .labelled_by(caption.id);
            });
            hint(ui, p, "Ctrl+Plus and Ctrl+Minus change it too.");
            section(ui, "Panels");
            changed |= ui.checkbox(&mut prefs.show_navigator, "Navigator (left)").changed();
            changed |= ui.checkbox(&mut prefs.show_insights, "Insights (right)").changed();
            changed |= ui.checkbox(&mut prefs.show_inspector, "Inspector (bottom)").changed();
            section(ui, "Storage");
            hint(ui, p, &format!("Crawls are saved in {}", paths::crawls_dir().display()));
            ui.horizontal(|ui| {
                if ui.button("Open crawls folder").clicked() {
                    open_folder(&paths::crawls_dir());
                }
                if ui.button("Open settings folder").clicked() {
                    open_folder(&paths::data_dir());
                }
            });
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new("Close").min_size(egui::vec2(80.0, 26.0))).clicked() {
                    self.open = false;
                }
            });
        });
        if !open {
            self.open = false;
        }
        changed
    }
}

pub fn open_folder(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
}

// ---- list mode --------------------------------------------------------------

#[derive(Default)]
pub struct ListDialog {
    pub open: bool,
    pub text: String,
}

impl ListDialog {
    /// Returns the URLs to crawl when the user confirms.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Vec<String>> {
        if !self.open {
            return None;
        }
        let mut result = None;
        let mut open = true;
        dialog("Upload URL list", ctx, [620.0, 420.0]).open(&mut open).show(ctx, |ui| {
            let p = theme::palette(ui);
            hint(ui, p, "One URL per line. Only these URLs are checked; their links are not followed.");
            ui.horizontal(|ui| {
                if ui.button("From file…").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Text or CSV", &["txt", "csv"]).pick_file() {
                        match std::fs::read_to_string(&path) {
                            Ok(t) => self.text = t,
                            Err(e) => self.text = format!("# Could not read {}: {e}", path.display()),
                        }
                    }
                if ui.button("Clear").clicked() {
                    self.text.clear();
                }
            });
            egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.text).font(egui::TextStyle::Monospace).desired_rows(14).desired_width(f32::INFINITY));
            });
            let urls = parse_list(&self.text);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(format!("{} URLs", crate::app::fmt_count(urls.len() as u64)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 26.0))).clicked() {
                        self.open = false;
                    }
                    if ui.add_enabled(!urls.is_empty(), egui::Button::new("Start crawl").min_size(egui::vec2(96.0, 26.0))).clicked() {
                        result = Some(urls.clone());
                        self.open = false;
                    }
                });
            });
        });
        if !open {
            self.open = false;
        }
        result
    }
}

/// Lines that look like http(s) URLs (the first CSV field), deduplicated.
pub fn parse_list(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .map(|l| l.split([',', '\t']).next().unwrap_or("").trim().trim_matches('"'))
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| url::Url::parse(l).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .map(|u| u.to_string())
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

// ---- help -------------------------------------------------------------------

#[derive(Default)]
pub struct HelpWindows {
    pub about: bool,
    pub shortcuts: bool,
    pub guide: bool,
    /// The logo for the About box, uploaded once (a texture is freed when
    /// its handle is dropped, so it must outlive the frame it is drawn in).
    logo: Option<egui::TextureHandle>,
}

const AUTHOR: &str = "Sukhdev Miyatra";
const LINKEDIN: &str = "https://www.linkedin.com/in/sukhdevmiyatra/";
const EMAIL: &str = "sukhdevmiyatra2@gmail.com";
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+O", "Open a crawl"),
    ("Ctrl+S", "Save a copy of the crawl"),
    ("Ctrl+W", "Close the crawl"),
    ("Ctrl+K", "Command palette: run anything by typing"),
    ("Ctrl+F", "Search the current view"),
    ("Ctrl+C", "Copy the selected URL"),
    ("Ctrl+Enter", "Start crawling"),
    ("Ctrl+Plus / Ctrl+Minus", "Make the interface bigger or smaller"),
    ("Ctrl+0", "Reset the interface size"),
    ("Up / Down", "Move through the table"),
    ("Escape", "Clear the selection"),
];

const GUIDE: &[(&str, &str)] = &[
    ("Audit a site", "Type the address in the box at the top and press Start. ShoutingMonkey follows links within the site, reads robots.txt and XML sitemaps when there are any, and checks images, scripts, stylesheets and external links. Sites without robots.txt or a sitemap crawl normally; Issues > Crawl notes says what was found. Pause holds every request that has not gone out; Stop ends the crawl and keeps what was found. Pressing Start during a crawl offers to stop it and start the new one."),
    ("Check a list of URLs", "Choose Mode > List, then upload or paste the addresses. Only those URLs are fetched."),
    ("Find your way", "The Navigator on the left groups every tab: Site, On-page, Indexing and Linking. Expand a tab to see its filters and how many URLs each holds; coloured squares mark issues. Press Ctrl+K to jump anywhere by typing."),
    ("Fix what matters first", "Insights > Issues lists every issue found, errors first, with a description and how to fix it. Click a severity to list only those, and click an issue to show the affected URLs. Drag the column edges to widen them."),
    ("Compare crawls", "Crawl the same site again later and open Insights > Crawl Compare: it compares with the earlier crawl and lists the URLs added to and removed from each issue."),
    ("Look at one URL", "Select a row. The pane at the bottom shows URL Details, Inlinks, Outlinks, Image Details, Resources, the SERP Snippet, View Source, HTTP Headers and Duplicate Details."),
    ("JavaScript sites", "Turn on Configuration > Rendering for sites that build content with scripts. Rendering uses the Edge or Chrome on this PC; the JavaScript tab then lists links and text that only appear after scripts run."),
    ("Export", "Bulk Export > Export Current Tab saves what the table shows. The other Bulk Export menus save links, images, issues, reports and XML sitemaps."),
    ("Crawl files", "Every crawl is saved in the crawls folder and can be reopened from File. File > Save As writes it elsewhere."),
];

impl HelpWindows {
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.about {
            let mut open = true;
            dialog("About ShoutingMonkey", ctx, [440.0, 330.0]).open(&mut open).show(ctx, |ui| {
                let p = theme::palette(ui);
                ui.horizontal(|ui| {
                    let logo = self.logo.get_or_insert_with(|| ui.ctx().load_texture("logo", crate::theme::icon_image(), Default::default()));
                    ui.image((logo.id(), egui::vec2(48.0, 48.0)));
                    ui.vertical(|ui| {
                        ui.heading("ShoutingMonkey");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    });
                });
                ui.add_space(8.0);
                ui.label("A free, open-source website crawler for technical SEO audits. Works offline; the only network traffic is the crawl itself.");
                ui.add_space(8.0);
                ui.label(format!("Made with \u{2665} by {AUTHOR}"));
                egui::Grid::new("about-links").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("GitHub").color(p.muted));
                    ui.hyperlink(REPOSITORY);
                    ui.end_row();
                    ui.label(RichText::new("LinkedIn").color(p.muted));
                    ui.hyperlink_to("linkedin.com/in/sukhdevmiyatra", LINKEDIN);
                    ui.end_row();
                    ui.label(RichText::new("Email").color(p.muted));
                    ui.hyperlink_to(EMAIL, format!("mailto:{EMAIL}"));
                    ui.end_row();
                });
                ui.add_space(8.0);
                hint(ui, p, "A hobby project: expect bugs, and double-check important findings. Report problems on GitHub. MIT licence; the libraries it uses are listed with their licences in THIRD_PARTY_LICENSES.txt.");
            });
            self.about &= open;
        }
        if self.shortcuts {
            let mut open = true;
            dialog("Keyboard Shortcuts", ctx, [420.0, 320.0]).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("keys").num_columns(2).striped(true).spacing([24.0, 6.0]).show(ui, |ui| {
                    for (k, what) in SHORTCUTS {
                        ui.label(RichText::new(*k).family(theme::semibold()));
                        ui.label(*what);
                        ui.end_row();
                    }
                });
            });
            self.shortcuts &= open;
        }
        if self.guide {
            let mut open = true;
            egui::Window::new("User Guide")
                .open(&mut open)
                .collapsible(false)
                .default_size([560.0, 480.0])
                .pivot(Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (title, body) in GUIDE {
                            section(ui, title);
                            ui.label(*body);
                            ui.add_space(4.0);
                        }
                    });
                });
            self.guide &= open;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parsing() {
        let urls = parse_list("https://a.com/\n# comment\nnot a url\nhttps://a.com/\n\"https://b.com/x\",200\nftp://c.com\n");
        assert_eq!(urls, vec!["https://a.com/", "https://b.com/x"]);
    }
}
