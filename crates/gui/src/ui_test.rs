//! End-to-end smoke tests of the real window, rendered offscreen. They
//! crawl a public scraping sandbox, so they are ignored by default:
//!
//!     SHOUTINGMONKEY_HOME=<scratch dir> cargo test -p gui -- --ignored --nocapture
//!
//! Screenshots are written to $SHOUTINGMONKEY_HOME/shots.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::model;
use crate::prefs::ThemeChoice;

fn save(h: &mut Harness<'_, App>, name: &str) {
    let dir = paths::data_dir().join("shots");
    std::fs::create_dir_all(&dir).unwrap();
    let img = h.render().expect("render");
    img.save(dir.join(format!("{name}.png"))).unwrap();
}

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).wgpu().build_eframe(|cc| App::new(cc));
    // The harness forces its own theme after start-up; restore the app's.
    let prefs = h.state().prefs.clone();
    theme::apply_prefs(&h.ctx, &prefs);
    h.step();
    h
}

/// Step the UI for a while, giving the worker threads time to answer.
fn settle(h: &mut Harness<'_, App>, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(40));
        h.step();
    }
}

#[test]
#[ignore = "crawls a public website; run manually"]
fn crawl_through_the_ui() {
    assert!(std::env::var_os("SHOUTINGMONKEY_HOME").is_some(), "set SHOUTINGMONKEY_HOME to a scratch folder");
    let _ = std::fs::remove_file(paths::prefs_path());

    let mut h = harness();
    settle(&mut h, 300);
    assert_eq!(h.state().prefs.theme, ThemeChoice::Light, "light is the default theme");
    save(&mut h, "01-empty");

    {
        let app = h.state_mut();
        app.cfg.max_urls = 120;
        app.cfg.rps_per_host = 10.0;
        app.address = "quotes.toscrape.com".into();
    }
    h.get_by_label("Start").click();
    settle(&mut h, 300);
    assert!(h.state().running(), "crawl should be running: {:?}", h.state().notice.as_ref().map(|n| &n.text));

    let deadline = Instant::now() + Duration::from_secs(150);
    let mut mid_saved = false;
    while h.state().running() {
        assert!(Instant::now() < deadline, "crawl took too long");
        settle(&mut h, 100);
        if !mid_saved && h.state().rows.len() > 25 {
            save(&mut h, "02-crawling");
            mid_saved = true;
        }
    }
    settle(&mut h, 1500);
    let notice = h.state().notice.as_ref().map(|n| n.text.clone()).unwrap_or_default();
    println!("finished: {notice}; {} rows", h.state().rows.len());
    assert!(notice.starts_with("Crawl finished"), "{notice}");
    assert!(h.state().counts.is_some(), "counts should have loaded");
    save(&mut h, "03-finished-overview");

    // Page Titles tab, Duplicate filter, first row selected.
    let (titles, shared) = model::by_slug("page_titles.duplicate").unwrap();
    h.state_mut().go_to(titles, shared);
    settle(&mut h, 600);
    let first = h.state().rows.first().map(|r| r.url.clone()).expect("duplicate titles rows");
    h.state_mut().select(Some(first));
    settle(&mut h, 600);
    assert!(h.state().detail.is_some(), "detail should load");
    save(&mut h, "04-page-titles-details");

    h.state_mut().detail_tab = DetailTab::Inlinks;
    h.state_mut().insight_tab = InsightTab::Problems;
    let first_issue = model::problems().next().map(|(t, f, _)| (t, f));
    h.state_mut().selected_problem = first_issue;
    settle(&mut h, 400);
    save(&mut h, "05-issues-inlinks");
    // The side tables scroll inside their panel; they never widen it.
    let before = h.state().table_rect.width();
    for _ in 0..300 {
        h.step();
    }
    let after = h.state().table_rect.width();
    assert!((before - after).abs() < 1.0, "the Issues table pushed the results table from {before} to {after} px");

    h.state_mut().detail_tab = DetailTab::Serp;
    h.state_mut().insight_tab = InsightTab::Structure;
    settle(&mut h, 1200);
    save(&mut h, "06a-structure-preview");
    h.state_mut().insight_tab = InsightTab::Breakdown;
    settle(&mut h, 1200);
    save(&mut h, "06-site-structure-serp");

    h.state_mut().insight_tab = InsightTab::Speed;
    h.state_mut().detail_tab = DetailTab::Headers;
    let (rc, _) = model::by_slug("response_codes.all").unwrap();
    h.state_mut().go_to(rc, 0);
    settle(&mut h, 800);
    save(&mut h, "07-response-times-headers");

    h.get_by_label("File").click();
    settle(&mut h, 300);
    save(&mut h, "08-file-menu");
    h.key_press(egui::Key::Escape);
    settle(&mut h, 200);

    let cfg = h.state().cfg.clone();
    h.state_mut().config_dialog.open_at(crate::dialogs::ConfigPage::Speed, &cfg);
    settle(&mut h, 300);
    save(&mut h, "09-configuration");
    h.state_mut().config_dialog.open = false;

    h.state_mut().diagram.open = true;
    h.state_mut().request_diagram();
    settle(&mut h, 4000);
    save(&mut h, "10-diagram");
    h.state_mut().diagram.open = false;

    h.state_mut().prefs.theme = ThemeChoice::Dark;
    let prefs = h.state().prefs.clone();
    theme::apply_prefs(&h.ctx, &prefs);
    settle(&mut h, 400);
    save(&mut h, "11-dark");
    h.state_mut().prefs.theme = ThemeChoice::Light;
    let prefs = h.state().prefs.clone();
    theme::apply_prefs(&h.ctx, &prefs);

    // The command palette finds actions and views by typing.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    settle(&mut h, 200);
    assert!(h.state().palette.is_open(), "Ctrl+K opens the palette");
    h.query_all_by_role(egui::Role::TextInput)
        .find(|n| n.is_focused())
        .expect("the palette's search box has focus")
        .type_text("missing title");
    settle(&mut h, 200);
    save(&mut h, "12-palette");
    h.key_press(egui::Key::Enter);
    settle(&mut h, 500);
    let (vt, ft) = model::by_slug("page_titles.missing").unwrap();
    assert_eq!((h.state().tab, h.state().filter), (vt, ft), "the palette jumped to Titles: Missing");

    // A second crawl of the same site, compared with the first.
    let first_db = h.state().db.clone().unwrap();
    std::thread::sleep(Duration::from_millis(1100)); // a new folder name (seconds resolution)
    h.state_mut().cfg.max_urls = 60;
    h.get_by_label("Start").click();
    settle(&mut h, 300);
    // Start during a crawl asks first; Cancel keeps the crawl going.
    if h.state().running() {
        h.get_by_label("Start").click();
        settle(&mut h, 200);
        save(&mut h, "12b-restart-prompt");
        h.get_by_label("Cancel").click();
        settle(&mut h, 200);
        assert!(h.state().running() || h.state().notice.is_some(), "Cancel leaves the crawl running");
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    while h.state().running() {
        assert!(Instant::now() < deadline, "second crawl took too long");
        settle(&mut h, 100);
    }
    h.state_mut().insight_tab = InsightTab::Changes;
    settle(&mut h, 3000);
    assert_eq!(h.state().baseline.as_ref(), Some(&first_db), "the earlier crawl is picked as the baseline");
    let cmp = h.state().comparison.as_ref().expect("comparison loaded");
    println!("changes: {} problems changed, {} pages gone", cmp.problems.len(), cmp.pages_removed);
    assert!(cmp.pages_removed > 0, "a smaller second crawl has fewer pages");
    h.state_mut().selected_change = Some(0);
    settle(&mut h, 300);
    save(&mut h, "13-changes");
}

/// A crawl file from an older version (old schema) opens and lists rows.
/// Set SHOUTINGMONKEY_OLD_DB to a copy of such a file.
#[test]
#[ignore = "needs an old crawl file; run manually"]
fn opens_old_crawl_file() {
    let db = std::path::PathBuf::from(std::env::var_os("SHOUTINGMONKEY_OLD_DB").expect("set SHOUTINGMONKEY_OLD_DB"));
    let mut h = harness();
    h.state_mut().open_crawl(db);
    settle(&mut h, 2500);
    let app = h.state();
    assert!(app.notice.is_none(), "{:?}", app.notice.as_ref().map(|n| &n.text));
    println!("old crawl: {} rows", app.rows.len());
    assert!(!app.rows.is_empty());
    save(&mut h, "18-old-crawl");
}

/// List mode, pause/resume, stop and clear, through the real controls.
#[test]
#[ignore = "crawls a public website; run manually"]
fn list_mode_pause_resume_stop() {
    let mut h = harness();
    settle(&mut h, 200);

    // List mode: exactly these URLs, including a broken one.
    h.state_mut().prefs.mode = crate::prefs::Mode::List;
    h.state_mut().list_urls = vec![
        "https://quotes.toscrape.com/".into(),
        "https://quotes.toscrape.com/page/2/".into(),
        "https://quotes.toscrape.com/login".into(),
        "https://quotes.toscrape.com/no-such-page-404".into(),
    ];
    h.get_by_label("Start").click();
    let deadline = Instant::now() + Duration::from_secs(60);
    settle(&mut h, 300);
    while h.state().running() {
        assert!(Instant::now() < deadline, "list crawl took too long");
        settle(&mut h, 100);
    }
    settle(&mut h, 1500);
    let urls: Vec<String> = h.state().rows.iter().map(|r| r.url.clone()).collect();
    println!("list mode rows: {urls:?}");
    assert_eq!(urls.len(), 4, "list mode must fetch only the listed URLs");
    let (rc, broken) = model::by_slug("response_codes.client_error").unwrap();
    let client_errors = h.state().counts.as_ref().unwrap().filters[model::flat_index(rc, broken)];
    assert_eq!(client_errors, 1, "the missing page is a 4xx");
    save(&mut h, "14-list-mode");

    // Spider mode, slowly, to pause, resume and stop it.
    h.state_mut().clear();
    h.state_mut().prefs.mode = crate::prefs::Mode::Spider;
    {
        let app = h.state_mut();
        app.cfg.rps_per_host = 4.0;
        app.cfg.max_urls = 400;
        app.address = "https://quotes.toscrape.com/".into();
    }
    h.get_by_label("Start").click();
    let deadline = Instant::now() + Duration::from_secs(60);
    while h.state().rows.len() < 8 {
        assert!(Instant::now() < deadline, "crawl did not start");
        settle(&mut h, 100);
    }
    h.get_by_label("Pause").click();
    settle(&mut h, 1500); // in-flight requests finish
    let snap = |h: &mut Harness<'_, App>| h.state_mut().run.as_mut().unwrap().snapshot();
    let paused_at = snap(&mut h).crawled;
    settle(&mut h, 2500);
    let still = snap(&mut h).crawled;
    println!("paused at {paused_at}, after 2.5 s {still}");
    assert_eq!(paused_at, still, "nothing is crawled while paused");
    assert!(h.state().run.as_ref().unwrap().is_paused());
    save(&mut h, "15-paused");

    h.get_by_label("Resume").click();
    settle(&mut h, 2500);
    assert!(snap(&mut h).crawled > still, "crawling continues after resume");
    h.get_by_label("Stop").click();
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.state().running() {
        assert!(Instant::now() < deadline, "stop took too long");
        settle(&mut h, 100);
    }
    settle(&mut h, 500);
    let notice = h.state().notice.as_ref().map(|n| n.text.clone()).unwrap_or_default();
    println!("after stop: {notice}");
    assert!(notice.starts_with("Crawl stopped"), "{notice}");
    let s = snap(&mut h);
    assert!(s.outcome.as_ref().unwrap().as_ref().unwrap().cancelled);
    save(&mut h, "16-stopped");

    h.state_mut().clear();
    settle(&mut h, 200);
    assert!(h.state().db.is_none());
    save(&mut h, "17-cleared");
}

/// Every menu, every action that does not open a system dialog or another
/// program, every tab and filter, and every Inspector tab, on a real crawl.
/// Fails on a panic, an action refused for no reason, or a query error.
#[test]
#[ignore = "crawls a public website; run manually"]
fn every_menu_and_action() {
    use crate::actions::Action;
    let mut h = harness();
    settle(&mut h, 200);
    {
        let app = h.state_mut();
        app.cfg.max_urls = 60;
        app.address = "quotes.toscrape.com".into();
    }
    h.get_by_label("Start").click();
    settle(&mut h, 300);
    let deadline = Instant::now() + Duration::from_secs(120);
    while h.state().running() {
        assert!(Instant::now() < deadline, "crawl took too long");
        settle(&mut h, 100);
    }
    settle(&mut h, 1500);

    for menu in ["File", "Edit", "View", "Mode", "Configuration", "Bulk Export", "Tools", "Help"] {
        h.get_by_label(menu).click();
        settle(&mut h, 150);
        save(&mut h, &format!("menu-{}", menu.to_lowercase().replace(' ', "-")));
        h.key_press(egui::Key::Escape);
        settle(&mut h, 100);
    }

    let first = h.state().rows.first().map(|r| r.url.clone()).expect("rows");
    h.state_mut().select(Some(first));
    settle(&mut h, 400);
    let external = |a: Action| {
        matches!(
            a,
            Action::Open | Action::SaveCopy | Action::Compare | Action::SaveProfile | Action::LoadProfile | Action::ExportView
                | Action::Export(_) | Action::OpenUrl | Action::OpenCrawlsFolder | Action::OpenLogFolder | Action::Exit
                | Action::Start | Action::PauseResume | Action::Stop | Action::Close
        )
    };
    for (n, a) in Action::all().into_iter().filter(|a| !external(*a)).enumerate() {
        h.state_mut().notice = None;
        let ctx = h.ctx.clone();
        h.state_mut().run(a, &ctx);
        settle(&mut h, 250);
        let label = a.label();
        if let Some(notice) = &h.state().notice {
            assert!(!notice.text.contains("not available"), "{label}: {}", notice.text);
        }
        save(&mut h, &format!("action-{n:02}"));
        println!("action-{n:02}: {label}");
        let app = h.state_mut();
        app.config_dialog.open = false;
        app.prefs_dialog.open = false;
        app.list_dialog.open = false;
        app.help = Default::default();
        app.diagram.open = false;
        app.palette.close();
        app.prefs.mode = crate::prefs::Mode::Spider;
        app.prefs.theme = ThemeChoice::Light;
        app.prefs.show_navigator = true;
        app.prefs.show_insights = true;
        app.prefs.show_inspector = true;
        let prefs = app.prefs.clone();
        theme::apply_prefs(&h.ctx, &prefs);
        let ctx = h.ctx.clone();
        h.state_mut().set_zoom(&ctx, 1.0);
        settle(&mut h, 100);
    }

    // Every tab and filter loads without a query error.
    for (vi, v) in VIEWS.iter().enumerate() {
        for fi in 0..v.filters.len() {
            h.state_mut().notice = None;
            h.state_mut().go_to(vi, fi);
            settle(&mut h, 120);
            if let Some(n) = &h.state().notice {
                assert!(!matches!(n.kind, NoticeKind::Error), "{}: {}", v.filters[fi].slug, n.text);
            }
        }
    }
    // Every Inspector and Insights tab, for a page with links.
    let (internal, html) = model::by_slug("internal.html").unwrap();
    h.state_mut().go_to(internal, html);
    settle(&mut h, 500);
    let page = h.state().rows.first().map(|r| r.url.clone()).expect("an HTML page");
    h.state_mut().select(Some(page));
    settle(&mut h, 500);
    for (k, t) in DetailTab::ALL.into_iter().enumerate() {
        h.state_mut().detail_tab = t;
        h.state_mut().insight_tab = InsightTab::ALL[k % InsightTab::ALL.len()];
        settle(&mut h, 400);
        save(&mut h, &format!("tab-{k}-{}", t.label().to_lowercase().replace(' ', "-")));
    }
}

/// The screenshots in README.md, written to docs/screenshots. They crawl a
/// public scraping sandbox; the status bar shows the crawl's result rather
/// than the file path, so no local folder names end up in the images.
#[test]
#[ignore = "crawls a public website; run manually to refresh the README screenshots"]
fn readme_screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |h: &mut Harness<'_, App>, name: &str, notice: &str| {
        h.state_mut().notice = Some(Notice { text: notice.to_string(), kind: NoticeKind::Info });
        // No mouse pointer left over the Start button in the pictures.
        h.event(egui::Event::PointerGone);
        settle(h, 300);
        h.render().expect("render").save(dir.join(format!("{name}.png"))).unwrap();
    };
    let crawl = |h: &mut Harness<'_, App>, max: u64| -> String {
        h.state_mut().cfg.max_urls = max;
        h.state_mut().address = "quotes.toscrape.com".into();
        h.get_by_label("Start").click();
        settle(h, 300);
        let deadline = Instant::now() + Duration::from_secs(240);
        while h.state().running() {
            assert!(Instant::now() < deadline, "crawl took too long");
            settle(h, 100);
        }
        settle(h, 1500);
        h.state().notice.as_ref().map(|n| n.text.clone()).unwrap_or_default()
    };

    let mut h = harness();
    settle(&mut h, 200);
    // An earlier, smaller crawl, so Crawl Compare has something to show.
    crawl(&mut h, 150);
    std::thread::sleep(Duration::from_millis(1100));
    let done = crawl(&mut h, 300);

    // Main window: Page Titles > Duplicate, an issue explained, inlinks.
    let (titles, dup) = model::by_slug("page_titles.duplicate").unwrap();
    h.state_mut().go_to_view(titles, dup);
    settle(&mut h, 600);
    let first = h.state().rows.first().map(|r| r.url.clone()).unwrap();
    h.state_mut().select(Some(first));
    h.state_mut().insight_tab = InsightTab::Problems;
    h.state_mut().selected_problem = model::by_slug("page_titles.duplicate");
    h.state_mut().detail_tab = DetailTab::Inlinks;
    shot(&mut h, "main-window", &done);

    // Response codes with URL details.
    let (rc, all) = model::by_slug("response_codes.all").unwrap();
    h.state_mut().go_to_view(rc, all);
    settle(&mut h, 600);
    let page = h.state().rows.iter().find(|r| r.url.ends_with("toscrape.com/")).map(|r| r.url.clone());
    h.state_mut().select(page);
    h.state_mut().detail_tab = DetailTab::UrlDetails;
    h.state_mut().insight_tab = InsightTab::Breakdown;
    shot(&mut h, "response-codes", &done);

    h.state_mut().detail_tab = DetailTab::Serp;
    h.state_mut().insight_tab = InsightTab::Structure;
    shot(&mut h, "serp-snippet", &done);

    h.state_mut().insight_tab = InsightTab::Changes;
    settle(&mut h, 3000);
    h.state_mut().selected_change = Some(0);
    shot(&mut h, "crawl-compare", &done);

    let cfg = h.state().cfg.clone();
    h.state_mut().config_dialog.open_at(crate::dialogs::ConfigPage::Crawl, &cfg);
    shot(&mut h, "configuration", &done);
    h.state_mut().config_dialog.open = false;

    h.state_mut().help.about = true;
    settle(&mut h, 200);
    shot(&mut h, "about", &done);
    h.state_mut().help.about = false;

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    settle(&mut h, 200);
    h.query_all_by_role(egui::Role::TextInput).find(|n| n.is_focused()).expect("palette search box").type_text("canonical");
    shot(&mut h, "command-palette", &done);
    h.key_press(egui::Key::Escape);

    h.state_mut().insight_tab = InsightTab::Problems;
    h.state_mut().detail_tab = DetailTab::Inlinks;
    h.state_mut().prefs.theme = ThemeChoice::Dark;
    let prefs = h.state().prefs.clone();
    theme::apply_prefs(&h.ctx, &prefs);
    h.state_mut().go_to_view(titles, dup);
    shot(&mut h, "dark-mode", &done);
    h.state_mut().prefs.theme = ThemeChoice::Light;
    let prefs = h.state().prefs.clone();
    theme::apply_prefs(&h.ctx, &prefs);
}
