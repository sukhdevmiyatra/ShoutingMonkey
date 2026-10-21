//! Runs one crawl on a background thread (with its own Tokio runtime) and
//! exposes its progress to the UI thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine::config::CrawlConfig;
use engine::crawl::{CrawlControl, CrawlSummary, Phase};

/// Latest known state of a running crawl.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub phase: Phase,
    pub crawled: u64,
    pub queued: u64,
    pub errors: u64,
    /// Requests running right now.
    pub in_flight: u64,
    pub current_url: Option<String>,
    /// Set once the crawl has ended.
    pub outcome: Option<Result<CrawlSummary, String>>,
}

/// The window the "current" URL/s rate is measured over.
const RATE_WINDOW: Duration = Duration::from_secs(5);

pub struct CrawlRun {
    pub started: Instant,
    /// Elapsed time frozen when the crawl ended.
    finished_after: Option<Duration>,
    max_urls: u64,
    /// Set by the UI once it has reported this run's end.
    pub end_reported: bool,
    state: Arc<Mutex<Snapshot>>,
    control: CrawlControl,
    samples: VecDeque<(Instant, u64)>,
}

impl CrawlRun {
    pub fn start(cfg: CrawlConfig, seeds: Vec<String>, db_path: PathBuf, ui: egui::Context) -> Self {
        let state = Arc::new(Mutex::new(Snapshot {
            phase: Phase::Discovering,
            crawled: 0,
            queued: 0,
            errors: 0,
            in_flight: 0,
            current_url: None,
            outcome: None,
        }));
        let control = CrawlControl::new();
        let max_urls = cfg.max_urls;

        let (thread_state, thread_control) = (state.clone(), control.clone());
        let spawned = std::thread::Builder::new().name("crawl".into()).spawn(move || {
            let result = run_blocking(cfg, seeds, db_path, thread_control, &thread_state, &ui);
            if let Err(e) = &result {
                tracing::error!("crawl failed: {e:#}");
            }
            let mut s = thread_state.lock().unwrap();
            s.phase = Phase::Finished;
            s.outcome = Some(result.map_err(|e| format!("{e:#}")));
            ui.request_repaint();
        });
        if let Err(e) = spawned {
            let mut s = state.lock().unwrap();
            s.phase = Phase::Finished;
            s.outcome = Some(Err(format!("could not start the crawl thread: {e}")));
        }

        Self {
            started: Instant::now(),
            finished_after: None,
            max_urls,
            end_reported: false,
            state,
            control,
            samples: VecDeque::new(),
        }
    }

    pub fn snapshot(&mut self) -> Snapshot {
        let s = self.state.lock().unwrap().clone();
        if s.outcome.is_some() && self.finished_after.is_none() {
            self.finished_after = Some(self.started.elapsed());
        }
        let now = Instant::now();
        if self.samples.back().is_none_or(|(t, _)| now.duration_since(*t) >= Duration::from_millis(250)) {
            self.samples.push_back((now, s.crawled));
        }
        while self.samples.front().is_some_and(|(t, _)| now.duration_since(*t) > RATE_WINDOW) {
            self.samples.pop_front();
        }
        s
    }

    pub fn is_running(&self) -> bool {
        self.state.lock().unwrap().outcome.is_none()
    }

    pub fn stop(&self) {
        self.control.resume();
        self.control.cancel();
    }

    pub fn stopping(&self) -> bool {
        self.control.is_cancelled() && self.is_running()
    }

    pub fn pause(&self) {
        self.control.pause();
    }

    pub fn resume(&self) {
        self.control.resume();
    }

    pub fn is_paused(&self) -> bool {
        self.control.is_paused()
    }

    pub fn elapsed(&self) -> Duration {
        self.finished_after.unwrap_or_else(|| self.started.elapsed())
    }

    /// URLs per second since the start.
    pub fn average_rate(&self, crawled: u64) -> f64 {
        crawled as f64 / self.elapsed().as_secs_f64().max(0.001)
    }

    /// URLs per second over the last few seconds.
    pub fn current_rate(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some((t0, c0)), Some((t1, c1))) if t1 > t0 => (c1 - c0) as f64 / t1.duration_since(*t0).as_secs_f64(),
            _ => 0.0,
        }
    }

    /// Done and known totals for the progress bar; the total is what is
    /// known so far (done + running + queued), capped by the URL limit.
    pub fn progress(&self, s: &Snapshot) -> (u64, u64) {
        let total = (s.crawled + s.in_flight + s.queued).min(self.max_urls).max(s.crawled);
        (s.crawled, total)
    }

    /// Share done, below 100% until the crawl has really ended.
    pub fn fraction(&self, s: &Snapshot) -> f32 {
        if s.outcome.is_some() {
            return 1.0;
        }
        let (done, total) = self.progress(s);
        (done as f32 / total.max(1) as f32).min(0.99)
    }
}

fn run_blocking(
    cfg: CrawlConfig,
    seeds: Vec<String>,
    db_path: PathBuf,
    control: CrawlControl,
    state: &Mutex<Snapshot>,
    ui: &egui::Context,
) -> anyhow::Result<CrawlSummary> {
    if let Some(dir) = db_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let crawl = tokio::spawn(engine::crawl::run(cfg, db_path, seeds, Some(tx), control));
        while let Some(p) = rx.recv().await {
            {
                let mut s = state.lock().unwrap();
                s.phase = p.phase;
                s.crawled = p.crawled;
                s.queued = p.queued;
                s.errors = p.errors;
                s.in_flight = p.in_flight;
                if p.current_url.is_some() {
                    s.current_url = p.current_url;
                }
            }
            ui.request_repaint_after(Duration::from_millis(100));
        }
        crawl.await?
    })
}
