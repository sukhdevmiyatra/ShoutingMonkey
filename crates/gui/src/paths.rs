//! Where the app keeps its settings, logs and crawls.

use std::path::{Path, PathBuf};

/// Set `SHOUTINGMONKEY_HOME` to keep settings, logs and crawls in one
/// folder instead (portable use, tests).
fn home_override() -> Option<PathBuf> {
    std::env::var_os("SHOUTINGMONKEY_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Per-user app data (settings, log): %LOCALAPPDATA%\ShoutingMonkey.
pub fn data_dir() -> PathBuf {
    home_override().unwrap_or_else(|| {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ShoutingMonkey")
    })
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.toml")
}

pub fn prefs_path() -> PathBuf {
    data_dir().join("ui.toml")
}

pub fn log_path() -> PathBuf {
    data_dir().join("shoutingmonkey.log")
}

/// Crawls live in Documents so people can find, copy and delete them.
pub fn crawls_dir() -> PathBuf {
    if let Some(home) = home_override() {
        return home.join("crawls");
    }
    dirs::document_dir()
        .unwrap_or_else(data_dir)
        .join("ShoutingMonkey crawls")
}

/// A new, unique folder name for a crawl of `host` started now.
pub fn new_crawl_dir(host: &str) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
    let safe: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') { c } else { '_' })
        .collect();
    crawls_dir().join(format!("{safe} {stamp}"))
}

/// The site and the date-time of a crawl folder name
/// ("example.com 2026-10-20 101500" → "example.com", "2026-10-20 101500").
fn split_folder(name: &str) -> (&str, &str) {
    let mut parts = name.rsplitn(3, ' ');
    let (time, date, host) = (parts.next(), parts.next(), parts.next());
    match (time, date, host) {
        (Some(t), Some(d), Some(h)) if t.len() == 6 && d.len() == 10 => (h, &name[h.len() + 1..]),
        _ => (name, ""),
    }
}


/// A crawl's display name: its folder's name.
pub fn crawl_name(db: &Path) -> String {
    db.parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| db.display().to_string())
}

/// Other crawls of the same site in the crawls folder, newest first,
/// excluding `db` itself and any newer than it. A site's `www.` and bare
/// addresses count as the same site, whichever was typed.
pub fn earlier_crawls(db: Option<&Path>) -> Vec<PathBuf> {
    let Some(db) = db else { return vec![] };
    let name = crawl_name(db);
    let (host, stamp) = split_folder(&name);
    let Ok(entries) = std::fs::read_dir(crawls_dir()) else { return vec![] };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            let (h, t) = split_folder(&folder);
            let candidate = e.path().join("crawl.db");
            let same_site = engine::norm::host_twins(h, host) || h == host;
            (same_site && !t.is_empty() && t < stamp && candidate.is_file()).then(|| (t.to_string(), candidate))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Open Explorer with `path` selected.
pub fn reveal_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_names() {
        assert_eq!(super::split_folder("example.com 2026-10-20 101500").0, "example.com");
        assert_eq!(super::split_folder("my site 2026-10-20 101500").0, "my site");
        assert_eq!(super::split_folder("loose").0, "loose");
        assert_eq!(super::split_folder("www.example.com 2026-10-20 101500"), ("www.example.com", "2026-10-20 101500"));
    }
}
