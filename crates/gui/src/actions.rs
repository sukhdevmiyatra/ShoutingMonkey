//! Every command the app offers, in one place. The menus and the command
//! palette (Ctrl+K) are both built from this list, so they never disagree.

use crate::app::{App, InsightTab, NoticeKind};
use crate::dialogs::{ConfigPage, open_folder};
use crate::model::VIEWS;
use crate::paths;
use crate::prefs::{Mode, ThemeChoice};
use crate::query::{ExportKind, sql};
use crate::theme;

/// A file export: menu label, save-dialog title and default file name.
pub struct ExportSpec {
    pub group: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub kind: fn() -> ExportKind,
}

pub const EXPORTS: &[ExportSpec] = &[
    ExportSpec { group: "Links", label: "All Inlinks", file: "links_by_target.csv", kind: || ExportKind::Sql(sql::ALL_INLINKS) },
    ExportSpec { group: "Links", label: "All Outlinks", file: "links_by_page.csv", kind: || ExportKind::Sql(sql::ALL_OUTLINKS) },
    ExportSpec { group: "Links", label: "All Anchor Text", file: "link_text.csv", kind: || ExportKind::Sql(sql::ALL_ANCHOR_TEXT) },
    ExportSpec { group: "Links", label: "Redirection (3xx) Inlinks", file: "links_to_redirects.csv", kind: || ExportKind::Sql(sql::REDIRECT_INLINKS) },
    ExportSpec { group: "Links", label: "Client Error (4xx) Inlinks", file: "links_to_broken_pages.csv", kind: || ExportKind::Sql(sql::CLIENT_ERROR_INLINKS) },
    ExportSpec { group: "Links", label: "Server Error (5xx) Inlinks", file: "links_to_server_errors.csv", kind: || ExportKind::Sql(sql::SERVER_ERROR_INLINKS) },
    ExportSpec { group: "Links", label: "No Response Inlinks", file: "links_no_response.csv", kind: || ExportKind::Sql(sql::NO_RESPONSE_INLINKS) },
    ExportSpec { group: "Images", label: "All Image Alt Text", file: "image_alt_text.csv", kind: || ExportKind::Sql(sql::IMAGE_ALT_TEXT) },
    ExportSpec { group: "Images", label: "Images Missing Alt Text", file: "images_without_alt.csv", kind: || ExportKind::Sql(sql::IMAGES_MISSING_ALT) },
    ExportSpec { group: "Issues", label: "Issues Overview", file: "problem_summary.csv", kind: || ExportKind::IssuesOverview },
    ExportSpec { group: "Issues", label: "All Issues (one file each)…", file: "", kind: || ExportKind::AllIssues },
    ExportSpec { group: "Reports", label: "Crawl Overview", file: "crawl_summary.csv", kind: || ExportKind::CrawlOverview },
    ExportSpec { group: "Reports", label: "All Redirects", file: "redirects.csv", kind: || ExportKind::AllRedirects },
    ExportSpec { group: "Reports", label: "Redirect Chains", file: "redirect_chains.csv", kind: || ExportKind::RedirectChains { errors_only: false } },
    ExportSpec { group: "Reports", label: "Redirects to Errors", file: "redirects_to_errors.csv", kind: || ExportKind::RedirectChains { errors_only: true } },
    ExportSpec { group: "Reports", label: "Non-Indexable Canonicals", file: "bad_canonicals.csv", kind: || ExportKind::Sql(sql::NON_INDEXABLE_CANONICALS) },
    ExportSpec { group: "Reports", label: "Insecure Content", file: "insecure_resources.csv", kind: || ExportKind::Sql(sql::INSECURE_CONTENT) },
    ExportSpec { group: "Reports", label: "Orphan URLs", file: "orphan_urls.csv", kind: || ExportKind::Sql(sql::ORPHAN_PAGES) },
    ExportSpec { group: "Reports", label: "SERP Summary", file: "titles_and_descriptions.csv", kind: || ExportKind::Sql(sql::SERP_SUMMARY) },
    ExportSpec { group: "Sitemaps", label: "XML Sitemap…", file: "sitemap.xml", kind: || ExportKind::XmlSitemap },
    ExportSpec { group: "Sitemaps", label: "Images Sitemap…", file: "image-sitemap.xml", kind: || ExportKind::ImageSitemap },
    ExportSpec { group: "Pages", label: "Page Source (one file each)…", file: "", kind: || ExportKind::PageSource },
];

pub const EXPORT_GROUPS: &[&str] = &["Links", "Images", "Issues", "Reports", "Sitemaps", "Pages"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    SaveCopy,
    Close,
    Settings,
    Exit,
    Find,
    CopyUrl,
    OpenUrl,
    Palette,
    ToggleNavigator,
    ToggleInsights,
    ToggleInspector,
    Theme(ThemeChoice),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ResetColumns,
    ResetLayout,
    Start,
    PauseResume,
    Stop,
    SpiderMode,
    ListMode,
    UploadList,
    Compare,
    Configure(ConfigPage),
    SaveProfile,
    LoadProfile,
    DefaultProfile,
    ExportView,
    Export(usize),
    CrawlMap,
    Insight(InsightTab),
    Guide,
    Shortcuts,
    OpenCrawlsFolder,
    OpenLogFolder,
    About,
}

impl Action {
    pub fn label(self) -> String {
        match self {
            Self::Open => "Open Crawl…".into(),
            Self::SaveCopy => "Save As…".into(),
            Self::Close => "Close Crawl".into(),
            Self::Settings => "Settings…".into(),
            Self::Exit => "Exit".into(),
            Self::Find => "Search".into(),
            Self::CopyUrl => "Copy URL".into(),
            Self::OpenUrl => "Open URL in Browser".into(),
            Self::Palette => "Command Palette…".into(),
            Self::ToggleNavigator => "Navigator".into(),
            Self::ToggleInsights => "Insights".into(),
            Self::ToggleInspector => "Inspector".into(),
            Self::Theme(t) => format!("Theme: {}", t.label()),
            Self::ZoomIn => "Zoom In".into(),
            Self::ZoomOut => "Zoom Out".into(),
            Self::ZoomReset => "Reset Zoom".into(),
            Self::ResetColumns => "Reset Columns".into(),
            Self::ResetLayout => "Reset Layout".into(),
            Self::Start => "Start".into(),
            Self::PauseResume => "Pause / Resume".into(),
            Self::Stop => "Stop".into(),
            Self::SpiderMode => "Spider (crawl a website)".into(),
            Self::ListMode => "List (check a list of URLs)".into(),
            Self::UploadList => "Upload List…".into(),
            Self::Compare => "Compare Crawls…".into(),
            Self::Configure(p) => format!("{}…", p.label()),
            Self::SaveProfile => "Save Configuration As…".into(),
            Self::LoadProfile => "Load Configuration…".into(),
            Self::DefaultProfile => "Clear Default Configuration".into(),
            Self::ExportView => "Export Current Tab…".into(),
            Self::Export(i) => EXPORTS[i].label.to_string(),
            Self::CrawlMap => "Crawl Tree Graph".into(),
            Self::Insight(t) => format!("Show {}", t.label()),
            Self::Guide => "User Guide".into(),
            Self::Shortcuts => "Keyboard Shortcuts".into(),
            Self::OpenCrawlsFolder => "Open Crawls Folder".into(),
            Self::OpenLogFolder => "Open Log Folder".into(),
            Self::About => "About ShoutingMonkey".into(),
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Open => "Ctrl+O",
            Self::SaveCopy => "Ctrl+S",
            Self::Close => "Ctrl+W",
            Self::Find => "Ctrl+F",
            Self::CopyUrl => "Ctrl+C",
            Self::Palette => "Ctrl+K",
            Self::ZoomIn => "Ctrl+Plus",
            Self::ZoomOut => "Ctrl+Minus",
            Self::ZoomReset => "Ctrl+0",
            Self::Start => "Ctrl+Enter",
            Self::Exit => "Alt+F4",
            _ => "",
        }
    }

    /// Whether the action can run now, or why not (shown, not hidden).
    pub fn blocked(self, app: &App) -> Option<&'static str> {
        let running = app.running();
        let has_db = app.db.is_some();
        match self {
            Self::Open | Self::SpiderMode | Self::ListMode | Self::UploadList | Self::LoadProfile if running => Some("a crawl is running"),
            Self::Close if running => Some("stop the crawl first"),
            Self::PauseResume | Self::Stop if !running => Some("no crawl is running"),
            Self::SaveCopy | Self::Close | Self::ExportView | Self::Export(_) | Self::CrawlMap | Self::Compare if !has_db => {
                Some("open or run a crawl first")
            }
            Self::CopyUrl | Self::OpenUrl if app.selected.is_none() => Some("select a URL first"),
            _ => None,
        }
    }

    /// Everything, for the command palette.
    pub fn all() -> Vec<Action> {
        let mut v = vec![
            Self::Start, Self::PauseResume, Self::Stop, Self::Open, Self::SaveCopy, Self::Close, Self::Compare,
            Self::SpiderMode, Self::ListMode, Self::UploadList, Self::ExportView, Self::Find, Self::CopyUrl, Self::OpenUrl,
            Self::CrawlMap, Self::ToggleNavigator, Self::ToggleInsights, Self::ToggleInspector,
        ];
        v.extend(ThemeChoice::ALL.map(Self::Theme));
        v.extend([Self::ZoomIn, Self::ZoomOut, Self::ZoomReset, Self::ResetColumns, Self::ResetLayout]);
        v.extend(ConfigPage::ALL.map(Self::Configure));
        v.extend([Self::SaveProfile, Self::LoadProfile, Self::DefaultProfile, Self::Settings]);
        v.extend((0..EXPORTS.len()).map(Self::Export));
        v.extend(InsightTab::ALL.map(Self::Insight));
        v.extend([Self::Guide, Self::Shortcuts, Self::OpenCrawlsFolder, Self::OpenLogFolder, Self::About]);
        v
    }

    /// Label with context, for the palette ("Bulk Export: Reports: All Redirects").
    pub fn palette_label(self) -> String {
        match self {
            Self::Export(i) => format!("Bulk Export: {}: {}", EXPORTS[i].group, EXPORTS[i].label),
            Self::Configure(p) => format!("Configuration: {}", p.label()),
            Self::ToggleNavigator | Self::ToggleInsights | Self::ToggleInspector => format!("Show or hide {}", self.label().to_lowercase()),
            _ => self.label(),
        }
    }
}

impl App {
    pub fn run(&mut self, action: Action, ctx: &egui::Context) {
        if let Some(why) = action.blocked(self) {
            return self.notify(format!("{} is not available: {why}.", action.label().trim_end_matches('…')), NoticeKind::Error);
        }
        match action {
            Action::Open => self.open_crawl_dialog(),
            Action::SaveCopy => self.save_copy_dialog(),
            Action::Close => self.clear(),
            Action::Settings => self.prefs_dialog.open = true,
            Action::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::Find => self.focus_search = true,
            Action::CopyUrl => {
                if let Some(u) = &self.selected {
                    ctx.copy_text(u.clone());
                }
            }
            Action::OpenUrl => {
                if let Some(u) = &self.selected {
                    ctx.open_url(egui::OpenUrl::new_tab(u));
                }
            }
            Action::Palette => self.palette.open(),
            Action::ToggleNavigator => self.toggle(|p| &mut p.show_navigator),
            Action::ToggleInsights => self.toggle(|p| &mut p.show_insights),
            Action::ToggleInspector => self.toggle(|p| &mut p.show_inspector),
            Action::Theme(t) => {
                self.prefs.theme = t;
                theme::apply_prefs(ctx, &self.prefs);
                self.prefs_dirty = true;
            }
            Action::ZoomIn => self.set_zoom(ctx, self.prefs.zoom + 0.1),
            Action::ZoomOut => self.set_zoom(ctx, self.prefs.zoom - 0.1),
            Action::ZoomReset => self.set_zoom(ctx, 1.0),
            Action::ResetColumns => {
                self.prefs.hidden_columns.clear();
                self.prefs.column_widths.clear();
                self.prefs_dirty = true;
            }
            Action::ResetLayout => {
                self.prefs.show_navigator = true;
                self.prefs.show_insights = true;
                self.prefs.show_inspector = true;
                self.prefs_dirty = true;
                self.layout_gen += 1;
                ctx.memory_mut(|m| m.reset_areas());
            }
            Action::Start => self.start_crawl(ctx),
            Action::PauseResume => self.pause_or_resume(),
            Action::Stop => self.stop_crawl(),
            Action::SpiderMode => self.set_mode(Mode::Spider),
            Action::ListMode => self.set_mode(Mode::List),
            Action::UploadList => {
                self.set_mode(Mode::List);
                self.list_dialog.open = true;
            }
            Action::Compare => self.choose_baseline_dialog(),
            Action::Configure(page) => self.config_dialog.open_at(page, &self.cfg),
            Action::SaveProfile => self.save_profile(),
            Action::LoadProfile => self.load_profile(),
            Action::DefaultProfile => {
                self.cfg = Default::default();
                self.save_settings();
                self.notify("Crawl settings restored to the defaults.", NoticeKind::Info);
            }
            Action::ExportView => self.export_current_view(),
            Action::Export(i) => {
                let e = &EXPORTS[i];
                self.export((e.kind)(), e.label.trim_end_matches('…'), e.file);
            }
            Action::CrawlMap => {
                self.diagram.open = true;
                self.request_diagram();
            }
            Action::Insight(t) => {
                self.prefs.show_insights = true;
                self.insight_tab = t;
            }
            Action::Guide => self.help.guide = true,
            Action::Shortcuts => self.help.shortcuts = true,
            Action::OpenCrawlsFolder => open_folder(&paths::crawls_dir()),
            Action::OpenLogFolder => open_folder(&paths::data_dir()),
            Action::About => self.help.about = true,
        }
    }

    fn toggle(&mut self, field: impl FnOnce(&mut crate::prefs::Prefs) -> &mut bool) {
        let f = field(&mut self.prefs);
        *f = !*f;
        self.prefs_dirty = true;
    }

    fn set_mode(&mut self, mode: Mode) {
        self.prefs.mode = mode;
        self.prefs_dirty = true;
    }

    /// Jump to a view and filter.
    pub fn go_to_view(&mut self, view: usize, filter: usize) {
        self.go_to(view, filter);
        self.nav_expanded.insert(view);
    }
}

/// Views and filters the palette can jump to: "Go to Page Titles: Missing".
pub fn destinations() -> Vec<(String, usize, usize)> {
    VIEWS
        .iter()
        .enumerate()
        .flat_map(|(vi, v)| {
            v.filters.iter().enumerate().map(move |(fi, f)| {
                let label = if fi == 0 { format!("Go to {}", v.title) } else { format!("Go to {}: {}", v.title, f.label) };
                (label, vi, fi)
            })
        })
        .collect()
}
