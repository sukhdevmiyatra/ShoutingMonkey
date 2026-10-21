//! ShoutingMonkey desktop app: a website crawler for SEO audits, with a
//! native window. Runs fully offline apart from the crawl's own requests.

// No console window in release builds: this is a GUI app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod charts;
mod details;
mod diagram;
mod dialogs;
mod grid;
mod logo;
mod main_view;
mod menus;
mod model;
mod palette;
mod paths;
mod prefs;
mod query;
mod runner;
mod side_panel;
mod theme;
mod toolbar;

fn main() {
    init_logging();
    // A GUI app has no console to print a panic to: log it and show it.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("ShoutingMonkey hit an unexpected error and has to close.\n\n{info}");
        tracing::error!("{msg}");
        show_error(&msg);
    }));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoutingMonkey")
            .with_inner_size([1200.0, 720.0])
            .with_min_inner_size([900.0, 560.0])
            .with_maximized(true)
            .with_icon(theme::window_icon()),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "ShoutingMonkey",
        options,
        Box::new(|cc| {
            let mut app = app::App::new(cc);
            // "Open with" / drag onto the exe: the first argument is a crawl file.
            if let Some(db) = std::env::args_os().nth(1).map(std::path::PathBuf::from) {
                app.open_crawl(db);
            }
            Ok(Box::new(app))
        }),
    ) {
        let msg = format!("ShoutingMonkey could not open its window.\n\n{e}\n\nLog: {}", paths::log_path().display());
        tracing::error!("{msg}");
        show_error(&msg);
    }
}

fn show_error(msg: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("ShoutingMonkey")
        .set_description(msg)
        .show();
}

/// Log warnings and errors to %LOCALAPPDATA%\ShoutingMonkey\shoutingmonkey.log.
fn init_logging() {
    let path = paths::log_path();
    let _ = std::fs::create_dir_all(paths::data_dir());
    // Start fresh when the log grows past 5 MB.
    let append = path.metadata().map(|m| m.len() < 5 * 1024 * 1024).unwrap_or(false);
    let file = std::fs::OpenOptions::new().create(true).append(append).write(true).truncate(!append).open(&path);
    if let Ok(file) = file {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into());
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .try_init();
    }
}
