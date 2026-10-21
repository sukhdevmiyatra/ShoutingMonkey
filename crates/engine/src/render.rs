//! JavaScript rendering through a locally installed Chromium browser
//! (Microsoft Edge or Google Chrome), driven headless over the DevTools
//! protocol. Pages are first fetched raw for status and headers; HTML pages
//! are then loaded in the browser and the rendered DOM replaces the raw HTML.

use std::path::PathBuf;
use std::time::Duration;

use chromiumoxide::{Browser, BrowserConfig};
use futures_util::StreamExt;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::config::CrawlConfig;

/// Most pages rendered at once, whatever the crawl concurrency: each tab
/// costs a renderer process.
const MAX_TABS: usize = 4;

pub struct Renderer {
    browser: Browser,
    handler: JoinHandle<()>,
    tabs: Semaphore,
    /// Extra time after the load event for scripts to fetch and draw content.
    settle: Duration,
    timeout: Duration,
    profile_dir: PathBuf,
}

/// The browser to use: the configured path if set, else Edge/Chrome
/// detection. `None` when no Chromium browser is installed.
pub fn find_browser(cfg: &CrawlConfig) -> Option<PathBuf> {
    let configured = cfg.browser_path.trim();
    if !configured.is_empty() {
        return Some(PathBuf::from(configured));
    }
    chromiumoxide::detection::default_executable(Default::default()).ok()
}

impl Renderer {
    pub async fn launch(cfg: &CrawlConfig) -> anyhow::Result<Self> {
        let exe = find_browser(cfg).ok_or_else(|| {
            anyhow::anyhow!(
                "JavaScript rendering needs Microsoft Edge or Google Chrome. \
                 Install one, set its path in settings, or turn rendering off."
            )
        })?;
        anyhow::ensure!(exe.is_file(), "browser not found at {}", exe.display());

        // A private profile per crawl: never touches the user's browser data,
        // and two crawls cannot fight over one profile lock.
        let profile_dir = std::env::temp_dir().join(format!(
            "shoutingmonkey-render-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let config = BrowserConfig::builder()
            .chrome_executable(&exe)
            .new_headless_mode()
            .user_data_dir(&profile_dir)
            .request_timeout(Duration::from_secs(cfg.timeout_secs.max(1) + 10))
            .args([
                format!("--user-agent={}", cfg.user_agent),
                // Images are not needed to build the DOM; skipping them is a big speedup.
                "--blink-settings=imagesEnabled=false".to_string(),
                "--mute-audio".to_string(),
                "--no-default-browser-check".to_string(),
            ])
            .build()
            .map_err(|e| anyhow::anyhow!("browser configuration: {e}"))?;

        let (browser, mut events) = Browser::launch(config)
            .await
            .map_err(|e| anyhow::anyhow!("could not start {}: {e}", exe.display()))?;
        // The handler must be polled for the browser connection to make progress.
        let handler = tokio::spawn(async move {
            while let Some(ev) = events.next().await {
                if let Err(e) = ev {
                    tracing::debug!("browser event error: {e}");
                }
            }
        });

        Ok(Self {
            browser,
            handler,
            tabs: Semaphore::new(cfg.concurrency.clamp(1, MAX_TABS)),
            settle: Duration::from_millis(cfg.render_wait_ms),
            timeout: Duration::from_secs(cfg.timeout_secs.max(1)) + Duration::from_millis(cfg.render_wait_ms),
            profile_dir,
        })
    }

    /// Load `url` in a fresh tab and return the rendered document's HTML.
    pub async fn render(&self, url: &str) -> anyhow::Result<String> {
        let _tab = self.tabs.acquire().await?;
        let page = self.browser.new_page("about:blank").await?;
        let rendered = tokio::time::timeout(self.timeout, async {
            page.goto(url).await?;
            tokio::time::sleep(self.settle).await;
            page.content().await
        })
        .await;
        let _ = page.close().await;
        match rendered {
            Ok(html) => Ok(html?),
            Err(_) => anyhow::bail!("rendering timed out after {}s", self.timeout.as_secs()),
        }
    }

    /// Close the browser and remove its temporary profile.
    pub async fn shutdown(mut self) {
        let _ = self.browser.close().await;
        let _ = self.browser.wait().await;
        self.handler.abort();
        // The browser may hold profile files briefly after exiting.
        for _ in 0..10 {
            if std::fs::remove_dir_all(&self.profile_dir).is_ok() || !self.profile_dir.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}
