//! The crawl driver: a coordinator keeps up to `concurrency` jobs running
//! in a `JoinSet`, popping from the shared frontier. Each job gates on
//! robots, paces per host, fetches, classifies, extracts, hands its record to
//! the database writer thread, and pushes discovered links back into the
//! frontier. The crawl ends when the frontier is empty and nothing is in
//! flight, the URL budget is spent, or it is cancelled.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::OnceCell;
use tokio::task::JoinSet;
use url::Url;

pub use tokio_util::sync::CancellationToken;

use crate::classify::{self, ResourceType};
use crate::config::CrawlConfig;
use crate::fetch::{self, Fetched};
use crate::frontier::{Frontier, Job, Via};
use crate::norm::{self, same_site};
use crate::parse::{self, LinkKind, PageData};
use crate::render::Renderer;
use crate::robots::{self, Robots};
use crate::sitemap;
use crate::store::{LinkRecord, Store, UrlRecord};

/// What the crawl is doing right now, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Reading robots.txt and sitemaps before the crawl proper.
    Discovering,
    Crawling,
    /// Paused: requests in flight finish, nothing new starts.
    Paused,
    /// Crawling the URLs only the sitemaps listed.
    SitemapUrls,
    /// Working out near duplicates and link scores after the crawl.
    Analysing,
    Finished,
}

/// Remote control for a running crawl: stop it, or pause and resume it.
/// Clones control the same crawl.
#[derive(Clone)]
pub struct CrawlControl {
    cancel: CancellationToken,
    paused: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for CrawlControl {
    fn default() -> Self {
        Self { cancel: CancellationToken::new(), paused: Arc::new(tokio::sync::watch::Sender::new(false)) }
    }
}

impl CrawlControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop the crawl: in-flight requests are aborted.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn pause(&self) {
        self.paused.send_replace(true);
    }

    pub fn resume(&self) {
        self.paused.send_replace(false);
    }

    pub fn is_paused(&self) -> bool {
        *self.paused.borrow()
    }

    async fn resumed(&self) {
        let mut rx = self.paused.subscribe();
        let _ = rx.wait_for(|paused| !*paused).await;
    }
}

/// Progress emitted to the UI layer while a crawl runs.
#[derive(Debug, Clone)]
pub struct CrawlProgress {
    pub phase: Phase,
    pub queued: u64,
    /// URLs processed so far (successful or not).
    pub crawled: u64,
    /// Processed URLs that failed: no response, or HTTP 4xx/5xx.
    pub errors: u64,
    /// Requests running right now.
    pub in_flight: u64,
    pub current_url: Option<String>,
}

pub type ProgressSink = tokio::sync::mpsc::UnboundedSender<CrawlProgress>;

/// How a crawl ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrawlSummary {
    pub crawled: u64,
    pub errors: u64,
    pub cancelled: bool,
}

/// Records awaiting the database writer thread.
type WriteItem = (UrlRecord, Vec<LinkRecord>);

/// Largest robots.txt read; RFC 9309 asks crawlers to parse at least 500 KiB.
const ROBOTS_MAX_BYTES: u64 = 512 * 1024;
const SITEMAP_MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Caps on sitemap discovery so a huge index cannot stall the start.
const MAX_SITEMAP_FILES: usize = 200;
const MAX_SITEMAP_INDEX_DEPTH: u32 = 2;
/// Records per database transaction.
const WRITE_BATCH: usize = 512;

/// Per-host pacing: delay seconds per host, next free request slot.
#[derive(Default)]
struct HostLimiter {
    delay: HashMap<String, f32>,
    next_slot: HashMap<String, Instant>,
}

impl HostLimiter {
    fn raise_delay(&mut self, host: &str, secs: f32) {
        if secs > 0.0 {
            let e = self.delay.entry(host.to_string()).or_insert(0.0);
            *e = e.max(secs);
        }
    }

    /// How long the caller must wait before requesting `host`. The slot is
    /// reserved immediately so concurrent workers queue up behind it.
    fn wait_for(&mut self, host: &str, fallback_delay: f32) -> Duration {
        let delay = self.delay.get(host).copied().unwrap_or(0.0).max(fallback_delay);
        if delay <= 0.0 {
            return Duration::ZERO;
        }
        let now = Instant::now();
        let slot = self.next_slot.get(host).copied().filter(|t| *t > now).unwrap_or(now);
        self.next_slot.insert(host.to_string(), slot + Duration::from_secs_f32(delay));
        slot - now
    }
}

/// The set of hosts that count as "internal". Starts with the seed hosts
/// and grows when a seed redirects to another host (example.com -> www).
struct Scope {
    hosts: RwLock<HashSet<String>>,
    include_subdomains: bool,
}

impl Scope {
    fn contains(&self, host: &str) -> bool {
        let hosts = self.hosts.read().unwrap();
        hosts.iter().any(|s| same_site(s, host, self.include_subdomains))
    }

    fn add(&self, host: String) {
        if !host.is_empty() {
            self.hosts.write().unwrap().insert(host);
        }
    }

    fn contains_url(&self, url: &str) -> bool {
        Url::parse(url).is_ok_and(|u| self.contains(&norm::host(&u)))
    }
}

type RobotsCell = Arc<OnceCell<Option<Arc<Robots>>>>;

/// Everything a job needs, shared by all running jobs.
struct Ctx {
    cfg: CrawlConfig,
    /// Page client: never follows redirects (they are crawl results).
    client: reqwest::Client,
    /// robots.txt / sitemap client: follows redirects.
    aux_client: reqwest::Client,
    scope: Scope,
    include: Vec<regex::Regex>,
    exclude: Vec<regex::Regex>,
    fallback_delay: f32,
    frontier: Mutex<Frontier>,
    limiter: Mutex<HostLimiter>,
    /// One fetch per origin; concurrent askers await the same cell.
    robots: Mutex<HashMap<String, RobotsCell>>,
    /// URLs listed in the site's sitemaps (normalized keys), with lastmod.
    sitemap_urls: Mutex<HashMap<String, Option<String>>>,
    /// Sitemap URLs to crawl once link discovery is done, if no link led there.
    sitemap_backlog: Mutex<Vec<String>>,
    /// `scheme://host/folder/` prefixes the crawl stays within.
    start_folders: RwLock<Vec<String>>,
    /// Set once the start page has pointed the crawl at the site's main
    /// address (see `main_address`), so it happens at most once.
    main_address_set: AtomicBool,
    /// What happened with robots.txt and sitemaps, for the crawl summary.
    diagnostics: Mutex<Vec<String>>,
    renderer: Option<Renderer>,
    writer: std_mpsc::Sender<WriteItem>,
    control: CrawlControl,
}

impl Ctx {
    async fn robots_for(&self, url: &Url) -> Option<Arc<Robots>> {
        let origin = norm::origin(url);
        let internal = self.scope.contains(&norm::host(url));
        let cell = self.robots.lock().unwrap().entry(origin.clone()).or_default().clone();
        cell.get_or_init(|| async {
            let (robots, note) = fetch_robots(&self.aux_client, origin.clone()).await;
            if internal {
                self.note(format!("robots.txt for {origin}: {note}"));
            }
            robots
        })
        .await
        .clone()
    }

    fn note(&self, line: String) {
        tracing::info!("{line}");
        self.diagnostics.lock().unwrap().push(line);
    }

    /// Whether a URL is inside a start folder (or folders do not matter).
    fn in_start_folder(&self, url: &str) -> bool {
        self.cfg.crawl_outside_start_folder || inside_folders(url, &self.start_folders.read().unwrap())
    }

    /// Cheap pre-fetch gate. Seeds (depth 0) are always crawled.
    fn admits(&self, job: &Job, url: &Url) -> bool {
        if job.depth == 0 {
            return true;
        }
        let internal = self.scope.contains(&norm::host(url));
        if !internal && self.cfg.restrict_to_seed_domain && !self.cfg.crawl_external {
            return false;
        }
        if !self.include.is_empty() && !self.include.iter().any(|re| re.is_match(&job.url)) {
            return false;
        }
        !self.exclude.iter().any(|re| re.is_match(&job.url))
    }

    fn write(&self, record: UrlRecord, links: Vec<LinkRecord>) -> anyhow::Result<()> {
        self.writer
            .send((record, links))
            .map_err(|_| anyhow::anyhow!("the crawl database writer stopped"))
    }

    fn enqueue(&self, discovered: Vec<Discovered>) {
        let mut f = self.frontier.lock().unwrap();
        for d in discovered {
            if d.depth <= self.cfg.max_depth {
                f.push_via(d.url, d.depth, d.redirect_count, d.via);
            }
        }
    }

    /// Queue the sitemap URLs no link led to. Returns how many were queued.
    fn release_sitemap_backlog(&self) -> usize {
        let backlog = std::mem::take(&mut *self.sitemap_backlog.lock().unwrap());
        let mut f = self.frontier.lock().unwrap();
        backlog.into_iter().filter(|u| f.push_via(u.clone(), 0, 0, Via::Sitemap)).count()
    }
}

/// Run a full crawl from `seed_urls` into the database at `db_path`.
/// Progress (if a sender is given) receives a snapshot per finished URL.
/// Cancelling through `control` aborts in-flight requests and returns
/// promptly; what was crawled so far stays in the database.
pub async fn run(
    cfg: CrawlConfig,
    db_path: PathBuf,
    seed_urls: Vec<String>,
    progress: Option<ProgressSink>,
    control: CrawlControl,
) -> anyhow::Result<CrawlSummary> {
    let cancel = control.cancel.clone();
    cfg.validate()?;
    let (include, exclude) = cfg.compile_patterns()?;

    let mut seeds = Vec::with_capacity(seed_urls.len());
    let mut seed_hosts = HashSet::new();
    for s in &seed_urls {
        let u = Url::parse(s.trim()).map_err(|e| anyhow::anyhow!("invalid seed URL {s:?}: {e}"))?;
        anyhow::ensure!(
            matches!(u.scheme(), "http" | "https") && u.host_str().is_some(),
            "seed URL must be http(s) with a host: {s}"
        );
        seed_hosts.insert(norm::host(&u));
        seeds.push(u);
    }
    anyhow::ensure!(!seeds.is_empty(), "no seed URL given");

    // Start the browser before touching the database so a missing browser
    // fails fast without leaving an empty crawl behind.
    let renderer = if cfg.render_js { Some(Renderer::launch(&cfg).await?) } else { None };

    let store = Store::open(&db_path)?;
    store.set_meta("config", &serde_json::to_string(&cfg)?)?;
    store.set_meta("seeds", &serde_json::to_string(&seed_urls)?)?;
    store.set_meta("started_at", &unix_now().to_string())?;
    store.set_meta("status", "running")?;
    let (writer, writer_thread) = spawn_writer(store)?;

    let fallback_delay = if cfg.rps_per_host > 0.0 { 1.0 / cfg.rps_per_host } else { 0.0 };
    let ctx = Arc::new(Ctx {
        client: fetch::build_client(&cfg, false)?,
        aux_client: fetch::build_client(&cfg, true)?,
        scope: Scope { hosts: RwLock::new(seed_hosts), include_subdomains: cfg.include_subdomains },
        include,
        exclude,
        fallback_delay,
        frontier: Mutex::new(Frontier::new()),
        limiter: Mutex::new(HostLimiter::default()),
        robots: Mutex::new(HashMap::new()),
        sitemap_urls: Mutex::new(HashMap::new()),
        sitemap_backlog: Mutex::new(Vec::new()),
        start_folders: RwLock::new(seeds.iter().map(start_folder).collect()),
        main_address_set: AtomicBool::new(false),
        diagnostics: Mutex::new(Vec::new()),
        renderer,
        writer,
        control: control.clone(),
        cfg,
    });
    {
        let mut f = ctx.frontier.lock().unwrap();
        for u in &seeds {
            f.push_via(norm::key(u), 0, 0, Via::Start);
        }
    }

    let report = |phase, crawled, errors, in_flight: usize, current_url: Option<String>| {
        if let Some(p) = &progress {
            let queued = ctx.frontier.lock().unwrap().queue_len() as u64 + ctx.sitemap_backlog.lock().unwrap().len() as u64;
            let _ = p.send(CrawlProgress { phase, queued, crawled, errors, in_flight: in_flight as u64, current_url });
        }
    };

    if ctx.cfg.follow_sitemaps {
        report(Phase::Discovering, 0, 0, 0, None);
        tokio::select! {
            n = read_sitemaps(&ctx, &seeds) => tracing::info!("sitemaps list {n} URLs"),
            _ = cancel.cancelled() => {}
        }
    } else if ctx.cfg.robots_reporting() {
        for seed in &seeds {
            ctx.robots_for(seed).await;
        }
    }
    let mut sitemap_phase = false;

    let concurrency = ctx.cfg.concurrency.max(1);
    let mut tasks: JoinSet<(String, anyhow::Result<bool>)> = JoinSet::new();
    let (mut dispatched, mut crawled, mut errors) = (0u64, 0u64, 0u64);
    let mut fatal: Option<anyhow::Error> = None;

    report(Phase::Crawling, 0, 0, 0, None);
    'crawl: loop {
        let phase = if sitemap_phase { Phase::SitemapUrls } else { Phase::Crawling };
        if control.is_paused() {
            report(Phase::Paused, crawled, errors, tasks.len(), None);
            if tasks.is_empty() {
                tokio::select! {
                    _ = control.resumed() => {}
                    _ = cancel.cancelled() => break 'crawl,
                }
                report(phase, crawled, errors, 0, None);
            }
        }
        // Fill the pool (not while paused: running requests just finish).
        while !cancel.is_cancelled()
            && !control.is_paused()
            && tasks.len() < concurrency
            && dispatched < ctx.cfg.max_urls
        {
            let Some(job) = ctx.frontier.lock().unwrap().pop() else { break };
            let Ok(url) = Url::parse(&job.url) else { continue };
            if !ctx.admits(&job, &url) {
                continue;
            }
            dispatched += 1;
            let ctx = ctx.clone();
            tasks.spawn(async move {
                let res = process_job(&ctx, &job, url).await;
                (job.url, res)
            });
        }
        if tasks.is_empty() {
            // Links are exhausted: crawl what only the sitemaps listed, once.
            if !sitemap_phase && !cancel.is_cancelled() && dispatched < ctx.cfg.max_urls {
                sitemap_phase = true;
                let n = ctx.release_sitemap_backlog();
                if n > 0 {
                    ctx.note(format!("Sitemaps: {n} URLs no link led to were crawled after the link crawl"));
                    continue;
                }
            }
            break; // frontier drained, budget spent, or cancelled
        }

        let joined = tokio::select! {
            j = tasks.join_next() => j,
            _ = cancel.cancelled() => break 'crawl,
        };
        let paused = control.is_paused();
        match joined {
            Some(Ok((url, Ok(is_error)))) => {
                crawled += 1;
                errors += u64::from(is_error);
                report(if paused { Phase::Paused } else { phase }, crawled, errors, tasks.len(), Some(url));
            }
            Some(Ok((url, Err(e)))) => {
                // Only storage failures surface here; the crawl cannot go on.
                fatal = Some(e.context(format!("while recording {url}")));
                break;
            }
            Some(Err(e)) => {
                crawled += 1;
                errors += 1;
                tracing::error!("crawl task failed: {e}");
            }
            None => {}
        }
    }

    tasks.shutdown().await;
    let queued = ctx.frontier.lock().unwrap().queue_len() as u64;
    // All tasks are finished, so this is the last handle on the context.
    // Dropping it (and with it the writer's sender) lets the writer drain.
    let ctx = Arc::into_inner(ctx).expect("crawl tasks still hold the context");
    if let Some(r) = ctx.renderer {
        r.shutdown().await;
    }
    drop(ctx.writer);
    let diagnostics = std::mem::take(&mut *ctx.diagnostics.lock().unwrap());
    let store = writer_thread
        .join()
        .map_err(|_| anyhow::anyhow!("the crawl database writer panicked"))??;
    store.set_meta("diagnostics", &serde_json::to_string(&diagnostics)?)?;
    if let Some(e) = fatal {
        let _ = store.set_meta("status", "failed");
        return Err(e);
    }

    let cancelled = cancel.is_cancelled();
    if let Some(p) = &progress {
        let _ = p.send(CrawlProgress { phase: Phase::Analysing, queued, crawled, errors, in_flight: 0, current_url: None });
    }
    if let Err(e) = crate::analysis::run(&store) {
        tracing::warn!("post-crawl analysis failed: {e:#}");
    }
    store.set_meta("finished_at", &unix_now().to_string())?;
    store.set_meta("status", if cancelled { "stopped" } else { "completed" })?;
    if let Some(p) = &progress {
        let _ = p.send(CrawlProgress { phase: Phase::Finished, queued, crawled, errors, in_flight: 0, current_url: None });
    }
    Ok(CrawlSummary { crawled, errors, cancelled })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Own the store on a dedicated thread, committing records in batches so
/// SQLite work never blocks the async workers. Returns the store when the
/// last sender is dropped.
fn spawn_writer(
    mut store: Store,
) -> anyhow::Result<(std_mpsc::Sender<WriteItem>, std::thread::JoinHandle<anyhow::Result<Store>>)> {
    let (tx, rx) = std_mpsc::channel::<WriteItem>();
    let handle = std::thread::Builder::new().name("crawl-db-writer".into()).spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut batch = vec![first];
            while batch.len() < WRITE_BATCH {
                match rx.try_recv() {
                    Ok(item) => batch.push(item),
                    Err(_) => break,
                }
            }
            store.record_batch(batch.iter().map(|(r, l)| (r, l.as_slice())))?;
        }
        Ok(store)
    })?;
    Ok((tx, handle))
}

/// Fetch, parse and record one URL; push its discoveries. Returns whether
/// the URL counts as an error. `Err` means the result could not be stored.
async fn process_job(ctx: &Ctx, job: &Job, url: Url) -> anyhow::Result<bool> {
    let cfg = &ctx.cfg;
    let host = norm::host(&url);
    let internal = ctx.scope.contains(&host);
    // Pages on other sites, and outside the start folder, are checked but
    // their links are not followed (unless the crawl is unbounded).
    let extract = (internal && ctx.in_start_folder(&job.url)) || !cfg.restrict_to_seed_domain;
    let base = UrlRecord {
        url: job.url.clone(),
        depth: job.depth,
        internal,
        found_via: job.via.as_str(),
        ..Default::default()
    };

    if job.redirect_count > cfg.max_redirects {
        ctx.write(
            UrlRecord {
                resource_type: ResourceType::NoResponse.slug(),
                error: Some(format!("more than {} redirects in a row", cfg.max_redirects)),
                indexability: "Too Many Redirects".into(),
                ..base
            },
            vec![],
        )?;
        return Ok(true);
    }

    let mut robots_blocked = false;
    if cfg.robots_reporting() {
        let robots_txt = ctx.robots_for(&url).await;
        let path_q = &url[url::Position::BeforePath..];
        robots_blocked = !robots::allows_for(robots_txt.as_deref(), &cfg.user_agent, path_q);
        if robots_blocked && cfg.respects_robots() {
            ctx.write(
                UrlRecord {
                    resource_type: ResourceType::NoResponse.slug(),
                    robots_blocked: true,
                    indexability: "Blocked by robots.txt".into(),
                    ..base
                },
                vec![],
            )?;
            return Ok(false);
        }
        if cfg.respects_robots()
            && let Some(d) = robots_txt.as_deref().and_then(|r| r.group_for(&cfg.user_agent)?.crawl_delay) {
                ctx.limiter.lock().unwrap().raise_delay(&host, d.max(0.0) as f32);
            }
    }

    // Pause holds every request that has not gone out yet, including ones
    // already waiting for their turn under the speed limit.
    ctx.control.resumed().await;
    let wait = ctx.limiter.lock().unwrap().wait_for(&host, ctx.fallback_delay);
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
    ctx.control.resumed().await;

    let want_html = |st: reqwest::StatusCode, ct: Option<&str>| {
        extract && st.is_success() && classify::classify(Some(st.as_u16()), ct, &url) == ResourceType::Html
    };
    let (mut rendered, mut render_error, mut raw_html) = (false, None, None);
    let outcome = match fetch::fetch(&ctx.client, &url, cfg.max_response_bytes, cfg.retries, want_html).await {
        Ok(mut f) => {
            if let Some(r) = ctx.renderer.as_ref().filter(|_| f.status == 200 && f.body.is_some()) {
                match r.render(&job.url).await {
                    Ok(html) => {
                        raw_html = f.body.replace(html);
                        rendered = true;
                    }
                    Err(e) => render_error = Some(e.to_string()),
                }
            }
            process_response(ctx, job, base, extract, f, &url)
        }
        Err(e) => UrlOutcome {
            record: UrlRecord {
                resource_type: ResourceType::NoResponse.slug(),
                error: Some(fetch::describe_error(&e)),
                indexability: if e.is_timeout() { "Timeout" } else { "No Response" }.into(),
                ..base
            },
            links: vec![],
            discovered: vec![],
        },
    };

    let mut record = outcome.record;
    record.robots_blocked = robots_blocked;
    record.rendered = rendered;
    record.render_error = render_error;
    if let (Some(raw), Some(page)) = (&raw_html, &record.page) {
        // What only exists once JavaScript has run.
        let before = parse::parse_html(raw, &url);
        let raw_links: HashSet<&str> = before.links.iter().filter(|l| l.kind == LinkKind::Anchor).map(|l| l.url.as_str()).collect();
        let js_links = page.links.iter().filter(|l| l.kind == LinkKind::Anchor && !raw_links.contains(l.url.as_str())).count();
        record.js_only_links = Some(js_links as i64);
        record.js_only_words = Some(page.word_count.saturating_sub(before.word_count) as i64);
    }
    if let Some(lastmod) = ctx.sitemap_urls.lock().unwrap().get(&job.url) {
        record.in_sitemap = true;
        record.sitemap_lastmod = lastmod.clone();
    }
    let is_error = record.error.is_some() || record.status.is_some_and(|s| s >= 400);
    ctx.write(record, outcome.links)?;
    ctx.enqueue(outcome.discovered);
    Ok(is_error)
}

struct UrlOutcome {
    record: UrlRecord,
    links: Vec<LinkRecord>,
    discovered: Vec<Discovered>,
}

struct Discovered {
    url: String,
    depth: u32,
    redirect_count: u32,
    via: Via,
}

/// Turn a fetched response into a record + links + discoveries.
fn process_response(ctx: &Ctx, job: &Job, base: UrlRecord, extract: bool, f: Fetched, url: &Url) -> UrlOutcome {
    let cfg = &ctx.cfg;
    let rt = classify::classify(Some(f.status), f.content_type.as_deref(), url);
    let header_noindex = parse::is_noindex(f.x_robots_tag.as_deref().unwrap_or(""));
    let base = UrlRecord {
        status: Some(f.status),
        content_type: f.content_type,
        resource_type: rt.slug(),
        size: f.size,
        response_ms: Some(f.elapsed_ms),
        x_robots_tag: f.x_robots_tag,
        headers: Some(f.headers),
        truncated: f.truncated,
        ..base
    };

    if rt == ResourceType::Redirect {
        let target = f.location.as_deref().and_then(|l| norm::resolve_key(url, l));
        let mut discovered = vec![];
        if let Some(t) = &target {
            // A seed that redirects to another host (example.com -> www.)
            // makes that host part of the site.
            if job.depth == 0
                && let Ok(tu) = Url::parse(t) {
                    ctx.scope.add(norm::host(&tu));
                }
            if job.depth == 0
                && let Ok(tu) = Url::parse(t)
            {
                ctx.start_folders.write().unwrap().push(start_folder(&tu));
            }
            // Redirect hops keep the depth: they are not link clicks.
            discovered.push(Discovered { url: t.clone(), depth: job.depth, redirect_count: job.redirect_count + 1, via: Via::Redirect });
        }
        return UrlOutcome {
            record: UrlRecord {
                redirect_to: target.or(f.location),
                indexability: "Redirected".into(),
                ..base
            },
            links: vec![],
            discovered,
        };
    }

    let Some(html) = f.body.filter(|_| rt == ResourceType::Html) else {
        // Non-HTML, error statuses, or external pages: headers only.
        let indexable = f.status == 200 && !header_noindex;
        return UrlOutcome {
            record: UrlRecord {
                indexable,
                indexability: indexability_label(f.status, header_noindex, false),
                ..base
            },
            links: vec![],
            discovered: vec![],
        };
    };

    let page: PageData = parse::parse_html(&html, url);
    let noindex = header_noindex || parse::is_noindex(page.meta_robots.as_deref().unwrap_or(""));
    let canonicalised = page.canonical.as_deref().is_some_and(|c| c != job.url);
    let indexable = f.status == 200 && !noindex && !canonicalised;

    // The start page answers on the site's other address (www or not, http
    // or https) without redirecting, and names the main address as its
    // canonical: the crawl moves there, so the result is the same whichever
    // address was typed. The typed address stays as one canonicalised page.
    let start_page = job.depth == 0 && matches!(job.via, Via::Start | Via::Redirect | Via::MetaRefresh);
    let moved_to = start_page
        .then(|| main_address(url, page.canonical.as_deref()?))
        .flatten()
        .filter(|_| !ctx.main_address_set.swap(true, Ordering::SeqCst));
    if let Some(main) = &moved_to {
        ctx.scope.add(norm::host(main));
        ctx.start_folders.write().unwrap().push(start_folder(main));
        ctx.note(format!(
            "Main address: {} answers without redirecting and names {} as its canonical, so the crawl continues on {}. \
             Redirect {} to {} so every page has one address.",
            job.url,
            norm::key(main),
            norm::origin(main),
            norm::origin(url),
            norm::origin(main)
        ));
    }

    let mut links = Vec::with_capacity(page.links.len());
    let mut discovered = vec![];
    if let Some(main) = &moved_to {
        discovered.push(Discovered { url: norm::key(main), depth: 0, redirect_count: job.redirect_count, via: Via::Start });
    }
    for l in &page.links {
        let to_internal = ctx.scope.contains_url(&l.url);
        let nofollow = l.nofollow || page.nofollow;
        links.push(LinkRecord {
            to_url: l.url.clone(),
            kind: l.kind.slug(),
            anchor: l.anchor.clone(),
            nofollow,
            internal: to_internal,
            flags: l.flags,
        });
        // Pages only the sitemap led to are read, but no depth can be given
        // to what they link to: the link crawl already found what it could.
        // After a move to the main address, this copy's links are not
        // followed: they lead to the same pages on the other address.
        if extract && moved_to.is_none() && job.via != Via::Sitemap && should_follow(cfg, l.kind, nofollow, to_internal) {
            let via = match l.kind {
                LinkKind::Anchor => Via::Link,
                LinkKind::Canonical => Via::Canonical,
                LinkKind::Hreflang => Via::Hreflang,
                _ => Via::Resource,
            };
            discovered.push(Discovered { url: l.url.clone(), depth: job.depth + 1, redirect_count: 0, via });
        }
    }
    // A meta refresh is followed like a redirect.
    if let Some(target) = &page.meta_refresh
        && extract
        && target != &job.url
    {
        discovered.push(Discovered { url: target.clone(), depth: job.depth, redirect_count: job.redirect_count + 1, via: Via::MetaRefresh });
    }

    UrlOutcome {
        record: UrlRecord {
            html: cfg.store_html.then_some(html),
            indexable,
            indexability: indexability_label(f.status, noindex, canonicalised),
            page: Some(page),
            ..base
        },
        links,
        discovered,
    }
}

/// The site's main address when a page's canonical is the same page on the
/// site's other host or scheme (`example.com` → `www.example.com`,
/// `http` → `https`). Canonicals to other paths or other sites are ordinary
/// canonicals, not a different address for the whole site.
fn main_address(page: &Url, canonical: &str) -> Option<Url> {
    let c = Url::parse(canonical).ok()?;
    let other_address = norm::origin(&c) != norm::origin(page) && norm::host_twins(&norm::host(page), &norm::host(&c));
    (other_address && c.path() == page.path()).then_some(c)
}

/// Whether a discovered link is queued for crawling/checking.
fn should_follow(cfg: &CrawlConfig, kind: LinkKind, nofollow: bool, internal: bool) -> bool {
    let external_ok = !internal && (cfg.crawl_external || !cfg.restrict_to_seed_domain);
    match kind {
        LinkKind::Anchor => {
            let nofollow_ok = !nofollow || (internal && cfg.follow_internal_nofollow);
            nofollow_ok && (internal || external_ok)
        }
        LinkKind::Image | LinkKind::Script | LinkKind::Stylesheet | LinkKind::Iframe => {
            cfg.check_resources && (internal || external_ok)
        }
        LinkKind::Canonical => internal && cfg.follow_canonicals,
        LinkKind::Hreflang => internal && cfg.follow_hreflang,
        // Meta refresh targets are queued as redirects by the caller.
        LinkKind::Redirect => false,
    }
}

fn indexability_label(status: u16, noindex: bool, canonicalised: bool) -> String {
    match status {
        200 if noindex => "Noindex",
        200 if canonicalised => "Canonicalised",
        200 => "Indexable",
        300..=399 => "Redirected",
        400..=499 => "Client Error",
        500..=599 => "Server Error",
        _ => "Non-200 Status",
    }
    .into()
}

/// robots.txt for an origin, and what happened, in words. `None` means no
/// restrictions: a missing file (4xx) allows everything. A file that cannot
/// be read (5xx or a network failure) does not stop the audit either; the
/// crawl goes ahead and the summary says why.
async fn fetch_robots(client: &reqwest::Client, origin: String) -> (Option<Arc<Robots>>, String) {
    let Ok(u) = Url::parse(&format!("{origin}/robots.txt")) else {
        return (None, "not checked (invalid address)".into());
    };
    match fetch::fetch(client, &u, ROBOTS_MAX_BYTES, 1, |st, _| st.is_success()).await {
        Ok(f) if (200..300).contains(&f.status) => {
            let r = robots::parse(f.body.as_deref().unwrap_or(""));
            let rules: usize = r.groups.iter().map(|g| g.allow.len() + g.disallow.len()).sum();
            let sitemaps = r.sitemap_urls().len();
            (Some(Arc::new(r)), format!("found, {rules} rules, {sitemaps} sitemaps declared"))
        }
        Ok(f) if f.status >= 500 => (None, format!("server error {}; crawled as if there were no rules", f.status)),
        Ok(f) => (None, format!("not found ({}); everything may be crawled", f.status)),
        Err(e) => (None, format!("could not be fetched ({}); crawled as if there were no rules", fetch::describe_error(&e))),
    }
}

/// The folder of a start URL: its path up to the last slash ("/blog/").
/// Only the path counts, so the www and bare host of a site, and included
/// subdomains, share the start folder.
fn start_folder(u: &Url) -> String {
    let path = u.path();
    match path.rfind('/') {
        Some(i) => path[..=i].to_string(),
        None => "/".to_string(),
    }
}

/// Whether `url`'s path is inside one of `folders`.
fn inside_folders(url: &str, folders: &[String]) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    folders.iter().any(|f| u.path().starts_with(f.as_str()))
}

/// Read the seeds' sitemaps (declared in robots.txt, else /sitemap.xml):
/// remember every listed URL with its lastmod, and keep the ones in scope as
/// a backlog for after the link crawl. Returns how many URLs are listed.
async fn read_sitemaps(ctx: &Ctx, seeds: &[Url]) -> usize {
    let mut pending: Vec<(String, u32)> = vec![];
    let mut origins_seen = HashSet::new();
    for seed in seeds {
        let origin = norm::origin(seed);
        if !origins_seen.insert(origin.clone()) {
            continue;
        }
        let declared = match ctx.robots_for(seed).await {
            Some(r) => r.sitemap_urls().to_vec(),
            None => vec![],
        };
        if declared.is_empty() {
            ctx.note(format!("Sitemaps: none declared in robots.txt for {origin}; trying {origin}/sitemap.xml"));
            pending.push((format!("{origin}/sitemap.xml"), 0));
        } else {
            pending.extend(declared.into_iter().map(|s| (s, 0)));
        }
    }

    let mut fetched_sitemaps = HashSet::new();
    let mut listed_total = 0;
    while let Some((sm_url, level)) = pending.pop() {
        if fetched_sitemaps.len() >= MAX_SITEMAP_FILES || !fetched_sitemaps.insert(sm_url.clone()) {
            continue;
        }
        let Ok(u) = Url::parse(&sm_url) else {
            ctx.note(format!("Sitemap {sm_url}: not a valid address"));
            continue;
        };
        let xml = match fetch::fetch(&ctx.aux_client, &u, SITEMAP_MAX_BYTES, 1, |st, _| st.is_success()).await {
            Ok(f) if (200..300).contains(&f.status) => f.body.unwrap_or_default(),
            Ok(f) => {
                ctx.note(format!("Sitemap {sm_url}: answered {}, not read", f.status));
                continue;
            }
            Err(e) => {
                ctx.note(format!("Sitemap {sm_url}: could not be fetched ({})", fetch::describe_error(&e)));
                continue;
            }
        };
        let doc = sitemap::parse(&xml);
        let what = if doc.sitemaps.is_empty() {
            format!("{} URLs", doc.urls.len())
        } else {
            format!("index of {} sitemaps", doc.sitemaps.len())
        };
        let warn = if doc.malformed { ", the XML is malformed" } else if doc.urls.is_empty() && doc.sitemaps.is_empty() { ", nothing listed" } else { "" };
        ctx.note(format!("Sitemap {sm_url}: {what}{warn}"));
        if level < MAX_SITEMAP_INDEX_DEPTH {
            pending.extend(doc.sitemaps.into_iter().map(|s| (s, level + 1)));
        }
        let mut listed = ctx.sitemap_urls.lock().unwrap();
        let mut backlog = ctx.sitemap_backlog.lock().unwrap();
        for (page, lastmod) in doc.urls.into_iter().zip(doc.lastmods) {
            let Ok(pu) = Url::parse(&page) else { continue };
            if !ctx.scope.contains(&norm::host(&pu)) {
                continue;
            }
            let key = norm::key(&pu);
            if listed.insert(key.clone(), lastmod).is_none() {
                listed_total += 1;
                backlog.push(key);
            }
        }
    }
    listed_total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn control_pause_resume() {
        let c = CrawlControl::new();
        assert!(!c.is_paused());
        c.pause();
        assert!(c.is_paused());
        let waiter = tokio::spawn({
            let c = c.clone();
            async move { c.resumed().await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished());
        c.resume();
        tokio::time::timeout(Duration::from_secs(1), waiter).await.unwrap().unwrap();
        c.cancel();
        assert!(c.is_cancelled());
    }

    #[test]
    fn main_address_is_the_canonical_twin() {
        let page = Url::parse("https://example.com/").unwrap();
        let m = |c: &str| main_address(&page, c).map(|u| u.to_string());
        assert_eq!(m("https://www.example.com/").as_deref(), Some("https://www.example.com/"));
        assert_eq!(m("http://example.com/"), Some("http://example.com/".into()), "scheme counts as another address");
        assert_eq!(m("https://example.com/"), None, "self-referencing");
        assert_eq!(m("https://www.example.com/home"), None, "another page");
        assert_eq!(m("https://other.com/"), None, "another site");
        assert_eq!(m("https://shop.example.com/"), None, "a subdomain is not the same address");
        let www = Url::parse("https://www.example.com/blog/").unwrap();
        assert_eq!(main_address(&www, "https://example.com/blog/").map(|u| u.to_string()).as_deref(), Some("https://example.com/blog/"));
    }

    #[test]
    fn start_folders() {
        let f = |s: &str| start_folder(&Url::parse(s).unwrap());
        assert_eq!(f("https://a.com"), "/");
        assert_eq!(f("https://a.com/blog/"), "/blog/");
        assert_eq!(f("https://a.com/blog/post?x=1"), "/blog/");
        assert_eq!(f("https://a.com/index.html"), "/");
        // The bare and www hosts of one site are both inside a root start.
        let root = vec![f("https://a.com/")];
        assert!(inside_folders("https://www.a.com/contact", &root));
        assert!(inside_folders("https://a.com/x/y", &root));
        let blog = vec![f("https://a.com/blog/")];
        assert!(inside_folders("https://www.a.com/blog/post", &blog));
        assert!(!inside_folders("https://a.com/about", &blog));
        assert!(!inside_folders("https://a.com/blogger", &blog), "a folder, not a prefix of a name");
    }

    #[test]
    fn limiter_spaces_requests() {
        let mut l = HostLimiter::default();
        assert_eq!(l.wait_for("a", 0.0), Duration::ZERO);
        assert_eq!(l.wait_for("a", 1.0), Duration::ZERO);
        let second = l.wait_for("a", 1.0);
        assert!(second > Duration::from_millis(900), "{second:?}");
        let third = l.wait_for("a", 1.0);
        assert!(third > Duration::from_millis(1900), "{third:?}");
        // other hosts are independent
        assert_eq!(l.wait_for("b", 1.0), Duration::ZERO);
    }

    #[test]
    fn follow_rules() {
        let cfg = CrawlConfig::default();
        assert!(should_follow(&cfg, LinkKind::Anchor, false, true));
        assert!(!should_follow(&cfg, LinkKind::Anchor, true, true));
        assert!(should_follow(&cfg, LinkKind::Anchor, false, false)); // external check
        assert!(should_follow(&cfg, LinkKind::Image, false, true));
        assert!(should_follow(&cfg, LinkKind::Canonical, false, true), "internal canonicals are crawled");
        assert!(!should_follow(&cfg, LinkKind::Canonical, false, false), "external canonicals are not");
        let no_canon = CrawlConfig { follow_canonicals: false, ..cfg.clone() };
        assert!(!should_follow(&no_canon, LinkKind::Canonical, false, true));
        let strict = CrawlConfig { crawl_external: false, check_resources: false, ..cfg.clone() };
        assert!(!should_follow(&strict, LinkKind::Anchor, false, false));
        assert!(!should_follow(&strict, LinkKind::Image, false, true));
        let nf = CrawlConfig { follow_internal_nofollow: true, ..cfg };
        assert!(should_follow(&nf, LinkKind::Anchor, true, true));
        assert!(!should_follow(&nf, LinkKind::Anchor, true, false));
    }

    #[test]
    fn labels() {
        assert_eq!(indexability_label(200, false, false), "Indexable");
        assert_eq!(indexability_label(200, true, true), "Noindex");
        assert_eq!(indexability_label(200, false, true), "Canonicalised");
        assert_eq!(indexability_label(404, false, false), "Client Error");
        assert_eq!(indexability_label(503, false, false), "Server Error");
    }

    #[test]
    fn scope_grows() {
        let s = Scope { hosts: RwLock::new(HashSet::from(["example.com".to_string()])), include_subdomains: false };
        assert!(!s.contains("shop.example.com"));
        s.add("shop.example.com".into());
        assert!(s.contains_url("https://shop.example.com/x"));
    }
}
