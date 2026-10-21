//! Interface preferences (not crawl settings), saved to ui.toml next to
//! settings.toml.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "Use Windows setting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// Crawl a site from a start URL.
    #[default]
    Spider,
    /// Check a fixed list of URLs.
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeChoice,
    /// Interface scale, 1.0 = 100%.
    pub zoom: f32,
    pub show_navigator: bool,
    #[serde(alias = "show_overview")]
    pub show_insights: bool,
    #[serde(alias = "show_details")]
    pub show_inspector: bool,
    pub mode: Mode,
    /// Most recent first.
    pub recent: Vec<PathBuf>,
    /// Hidden column ids per tab id.
    pub hidden_columns: BTreeMap<String, Vec<String>>,
    /// Column widths the user dragged, keyed "tab/column".
    pub column_widths: BTreeMap<String, f32>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Light,
            zoom: 1.0,
            show_navigator: true,
            show_insights: true,
            show_inspector: true,
            mode: Mode::Spider,
            recent: vec![],
            hidden_columns: BTreeMap::new(),
            column_widths: BTreeMap::new(),
        }
    }
}

const RECENT_LIMIT: usize = 10;

impl Prefs {
    pub fn load() -> Self {
        std::fs::read_to_string(paths::prefs_path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::prefs_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add_recent(&mut self, db: PathBuf) {
        self.recent.retain(|p| p != &db);
        self.recent.insert(0, db);
        self.recent.truncate(RECENT_LIMIT);
    }

    pub fn is_hidden(&self, tab: &str, column: &str) -> bool {
        self.hidden_columns.get(tab).is_some_and(|c| c.iter().any(|c| c == column))
    }

    /// The column's width: the user's, or the model's default.
    pub fn column_width(&self, tab: &str, column: &str) -> f32 {
        self.column_widths
            .get(&format!("{tab}/{column}"))
            .copied()
            .unwrap_or_else(|| crate::model::col(column).width)
    }

    pub fn set_column_width(&mut self, tab: &str, column: &str, width: f32) {
        self.column_widths.insert(format!("{tab}/{column}"), width);
    }

    /// Show every column of a tab at its default width.
    pub fn reset_columns(&mut self, tab: &str) {
        self.hidden_columns.remove(tab);
        let prefix = format!("{tab}/");
        self.column_widths.retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn set_hidden(&mut self, tab: &str, column: &str, hidden: bool) {
        let list = self.hidden_columns.entry(tab.to_string()).or_default();
        list.retain(|c| c != column);
        if hidden {
            list.push(column.to_string());
        }
    }
}
