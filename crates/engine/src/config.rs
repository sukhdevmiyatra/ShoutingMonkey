use serde::{Deserialize, Serialize};
use std::path::Path;

/// How robots.txt is treated: Respect (obey its rules, including
/// crawl-delay), Ignore (never consult it), or IgnoreButReport (crawl
/// everything but flag the URLs it disallows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RobotsMode {
    #[default]
    Respect,
    Ignore,
    IgnoreButReport,
}

impl RobotsMode {
    pub const ALL: [RobotsMode; 3] = [Self::Respect, Self::IgnoreButReport, Self::Ignore];

    pub fn label(self) -> &'static str {
        match self {
            Self::Respect => "Respect robots.txt",
            Self::IgnoreButReport => "Ignore, but report blocked URLs",
            Self::Ignore => "Ignore robots.txt",
        }
    }
}

/// Crawl configuration. Serialized to TOML; CLI/GUI overrides overlay on top.
/// Unknown keys (e.g. from older versions) are ignored on load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrawlConfig {
    /// Settings format; older files are upgraded on load (see `upgrade`).
    pub config_version: u32,
    pub max_urls: u64,
    pub max_depth: u32,
    pub concurrency: usize,
    /// Requests per second per host; 0 disables pacing.
    pub rps_per_host: f32,
    pub timeout_secs: u64,
    /// Extra attempts on network errors, 429 and 5xx.
    pub retries: usize,
    pub max_redirects: u32,
    pub user_agent: String,
    pub robots_mode: RobotsMode,
    pub include_subdomains: bool,
    /// When false, the crawl follows links onto any host (spidering the web).
    pub restrict_to_seed_domain: bool,
    /// Check (fetch status of, but never follow) links to other domains.
    pub crawl_external: bool,
    /// Check internal images, scripts, stylesheets and iframes.
    pub check_resources: bool,
    /// Enqueue internal links with rel=nofollow.
    pub follow_internal_nofollow: bool,
    pub max_response_bytes: u64,
    pub store_html: bool,
    /// Read XML sitemaps (robots.txt `Sitemap:` lines, then /sitemap.xml):
    /// marks listed URLs, and crawls the ones no link led to after the link
    /// crawl has finished.
    pub follow_sitemaps: bool,
    /// Crawl the URL a page's canonical tag names.
    pub follow_canonicals: bool,
    /// Crawl the URLs a page's hreflang links name.
    pub follow_hreflang: bool,
    /// Crawl pages outside the start URL's folder (e.g. above /blog/).
    pub crawl_outside_start_folder: bool,
    /// Render HTML pages in a headless browser so JavaScript-built content
    /// and links are seen.
    pub render_js: bool,
    /// Wait after the page's load event for scripts to finish, in ms.
    pub render_wait_ms: u64,
    /// Browser executable for rendering; empty = detect Edge or Chrome.
    pub browser_path: String,
    /// Only crawl URLs matching one of these regexes (seeds are exempt).
    pub include_patterns: Vec<String>,
    /// Never crawl URLs matching any of these regexes (seeds are exempt).
    pub exclude_patterns: Vec<String>,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            max_urls: 5_000_000,
            // Effectively unlimited, as crawlers usually are by default.
            max_depth: 1000,
            concurrency: 5,
            // No pacing beyond the thread count; set a limit for fragile sites.
            rps_per_host: 0.0,
            timeout_secs: 20,
            retries: 1,
            max_redirects: 5,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 shoutingmonkey/0.1".to_string(),
            robots_mode: RobotsMode::Respect,
            include_subdomains: false,
            restrict_to_seed_domain: true,
            crawl_external: true,
            check_resources: true,
            follow_internal_nofollow: false,
            max_response_bytes: 50 * 1024 * 1024,
            store_html: false,
            follow_sitemaps: true,
            follow_canonicals: true,
            follow_hreflang: true,
            crawl_outside_start_folder: false,
            render_js: false,
            render_wait_ms: 1500,
            browser_path: String::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }
}

/// Current settings format. Version 2 changed the speed and depth defaults.
pub const CONFIG_VERSION: u32 = 2;

impl CrawlConfig {
    /// Bring settings saved by an older version up to date: values that were
    /// left at an old default move to the new default; choices the user made
    /// are kept.
    pub fn upgrade(&mut self) {
        if self.config_version < 2 {
            if self.rps_per_host == 2.0 {
                self.rps_per_host = 0.0;
            }
            if self.concurrency == 16 {
                self.concurrency = 5;
            }
            if self.max_depth == 10 {
                self.max_depth = 1000;
            }
            if self.max_redirects == 10 {
                self.max_redirects = 5;
            }
        }
        self.config_version = CONFIG_VERSION;
    }

    pub fn sample_toml() -> String {
        let mut s = toml::to_string_pretty(&Self::default()).expect("serialize default config");
        s.insert_str(
            0,
            "# shoutingmonkey crawl configuration\n\
             # clone this file, tweak, pass with: shoutingmonkey-cli crawl <url> --config my.toml\n\n",
        );
        s
    }

    pub fn respects_robots(&self) -> bool {
        self.robots_mode == RobotsMode::Respect
    }

    /// Whether robots.txt is fetched at all (to obey it or to report on it).
    pub fn robots_reporting(&self) -> bool {
        self.robots_mode != RobotsMode::Ignore
    }

    /// Reject settings the crawl cannot run with, with a readable reason.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.max_urls > 0, "max URLs must be at least 1");
        anyhow::ensure!(self.concurrency > 0, "concurrency must be at least 1");
        anyhow::ensure!(
            self.rps_per_host.is_finite() && self.rps_per_host >= 0.0,
            "requests per second must be 0 or more"
        );
        anyhow::ensure!(self.timeout_secs > 0, "timeout must be at least 1 second");
        anyhow::ensure!(!self.user_agent.trim().is_empty(), "user agent must not be empty");
        self.compile_patterns()?;
        Ok(())
    }

    /// Compile include/exclude patterns, naming the first invalid one.
    pub fn compile_patterns(&self) -> anyhow::Result<(Vec<regex::Regex>, Vec<regex::Regex>)> {
        let compile = |kind: &str, pats: &[String]| {
            pats.iter()
                .filter(|p| !p.trim().is_empty())
                .map(|p| {
                    regex::Regex::new(p.trim())
                        .map_err(|e| anyhow::anyhow!("invalid {kind} pattern {p:?}: {e}"))
                })
                .collect::<anyhow::Result<Vec<_>>>()
        };
        Ok((
            compile("include", &self.include_patterns)?,
            compile("exclude", &self.exclude_patterns)?,
        ))
    }

    /// Load settings, upgrading ones saved by an older version.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let value: toml::Table = toml::from_str(&text)?;
        let mut cfg: Self = value.clone().try_into()?;
        if !value.contains_key("config_version") {
            cfg.config_version = 1;
        }
        cfg.upgrade();
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_roundtrips() {
        let text = CrawlConfig::sample_toml();
        let cfg: CrawlConfig = toml::from_str(&text).unwrap();
        assert_eq!(cfg, CrawlConfig::default());
    }

    #[test]
    fn old_defaults_are_upgraded() {
        let dir = std::env::temp_dir().join(format!("sm-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.toml");
        std::fs::write(&path, "rps_per_host = 2.0\nconcurrency = 16\nmax_depth = 3\n").unwrap();
        let cfg = CrawlConfig::load(&path).unwrap();
        assert_eq!(cfg.rps_per_host, 0.0, "old default speed limit is lifted");
        assert_eq!(cfg.concurrency, 5);
        assert_eq!(cfg.max_depth, 3, "a depth the user chose is kept");
        assert_eq!(cfg.config_version, CONFIG_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_keys_are_ignored() {
        let cfg: CrawlConfig =
            toml::from_str("max_urls = 7\nrender_mode = \"Never\"\nscreenshots = false\n").unwrap();
        assert_eq!(cfg.max_urls, 7);
    }

    #[test]
    fn validate_reports_bad_regex() {
        let cfg = CrawlConfig { exclude_patterns: vec!["(".into()], ..Default::default() };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("exclude"), "{err}");
        assert!(CrawlConfig::default().validate().is_ok());
    }
}
