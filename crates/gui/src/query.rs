//! Background query worker. All SQLite reads and exports run on one thread
//! so the UI never waits on the database, even while a crawl writes to it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use engine::store::{Store, Table, value_text};
use rusqlite::types::Value;

use crate::model::{self, COLS, VIEWS};

/// Rows beyond this are not loaded into the table (exports include all).
pub const ROW_LIMIT: usize = 200_000;
/// Most pages drawn in the crawl diagram.
pub const DIAGRAM_LIMIT: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: &'static str,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub db: PathBuf,
    pub tab: usize,
    pub filter: usize,
    pub search: String,
    pub sort: Option<Sort>,
}

impl View {
    fn where_clause(&self) -> String {
        format!(
            "{} AND (?1 = '' OR url LIKE ?2 ESCAPE '\\' OR COALESCE(title, '') LIKE ?2 ESCAPE '\\')",
            model::condition(self.tab, self.filter)
        )
    }

    fn order_clause(&self) -> String {
        match self.sort {
            // NULLs last either way: empty cells are rarely what you sort for.
            Some(s) => {
                let e = model::col(s.column).expr;
                format!("({e}) IS NULL, ({e}) {}, id", if s.descending { "DESC" } else { "ASC" })
            }
            None => "id".into(),
        }
    }

    fn like_pattern(&self) -> String {
        let escaped = self.search.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    }

    fn select_sql(&self, limit: Option<usize>) -> String {
        let cols: Vec<String> = VIEWS[self.tab].columns.iter().map(|c| model::col(c).expr.to_string()).collect();
        let mut sql = format!(
            "SELECT {} FROM urls WHERE {} ORDER BY {}",
            cols.join(", "),
            self.where_clause(),
            self.order_clause()
        );
        if let Some(n) = limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }
        sql
    }

    fn run(&self, store: &Store, limit: Option<usize>) -> anyhow::Result<Table> {
        let search = self.search.trim().to_string();
        let mut t = store.query(&self.select_sql(limit), &[&search, &self.like_pattern()])?;
        t.columns = VIEWS[self.tab].columns.iter().map(|c| model::col(c).title.to_string()).collect();
        Ok(t)
    }
}

pub struct Row {
    pub url: String,
    pub status: Option<i64>,
    pub cells: Vec<String>,
}

pub struct RowsReply {
    pub view: View,
    pub rows: Vec<Row>,
    pub truncated: bool,
}

pub struct Counts {
    pub db: PathBuf,
    /// One per (tab, filter), see `model::flat_index`.
    pub filters: Vec<u64>,
    pub total: u64,
}

#[derive(Default)]
pub struct DirNode {
    pub path: String,
    pub count: u64,
    pub children: Vec<DirNode>,
}

pub struct Stats {
    pub db: PathBuf,
    /// (depth, URLs); the last bucket holds depth 10 and deeper.
    pub depth: Vec<(u32, u64)>,
    /// HTML pages per response-time second: 0–1 s, 1–2 s, ... 10 s+.
    pub response_times: [u64; 11],
    pub directories: Vec<DirNode>,
    /// What the crawler noted about robots.txt and sitemaps.
    pub notes: Vec<String>,
    /// Pages reached only through a sitemap (they have no click depth).
    pub sitemap_only: u64,
}

pub struct Detail {
    pub url: String,
    pub fields: Vec<(String, String)>,
    pub inlinks: Table,
    pub outlinks: Table,
    pub images: Table,
    pub resources: Table,
    pub duplicates: Table,
    pub headers: Vec<(String, String)>,
    pub source: Option<String>,
    pub serp: Serp,
}

#[derive(Default)]
pub struct Serp {
    pub title: String,
    pub description: String,
    pub title_px: u32,
    pub description_px: u32,
}

pub struct DiagramNode {
    pub url: String,
    pub depth: u32,
    pub status: Option<i64>,
    pub indexable: bool,
    pub inlinks: u64,
    pub parent: Option<usize>,
}

pub struct Diagram {
    pub db: PathBuf,
    pub nodes: Vec<DiagramNode>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub enum ExportKind {
    View(View),
    Sql(&'static str),
    AllIssues,
    CrawlOverview,
    IssuesOverview,
    AllRedirects,
    RedirectChains { errors_only: bool },
    PageSource,
    XmlSitemap,
    ImageSitemap,
}

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub db: PathBuf,
    pub kind: ExportKind,
    /// A file, or a folder for the multi-file exports.
    pub target: PathBuf,
}

/// How one problem changed between an earlier crawl and this one.
pub struct ProblemDelta {
    pub view: usize,
    pub filter: usize,
    pub before: u64,
    pub now: u64,
    /// URLs with the problem now that did not have it before.
    pub new: Vec<String>,
    /// URLs that had it before, are still crawled, and no longer have it.
    pub fixed: Vec<String>,
}

pub struct Comparison {
    pub db: PathBuf,
    pub baseline: PathBuf,
    pub pages_added: u64,
    pub pages_removed: u64,
    /// Problems that changed, biggest change first.
    pub problems: Vec<ProblemDelta>,
}

pub enum Request {
    Compare { db: PathBuf, baseline: PathBuf },
    Rows(View),
    Counts(PathBuf),
    Stats(PathBuf),
    Detail { db: PathBuf, url: String },
    Diagram(PathBuf),
    Export(ExportJob),
    SaveCopy { db: PathBuf, target: PathBuf },
}

pub enum Reply {
    Rows(Result<RowsReply, String>),
    Counts(Result<Counts, String>),
    Stats(Result<Stats, String>),
    Detail(Result<Box<Detail>, String>),
    Diagram(Result<Diagram, String>),
    Compare(Result<Box<Comparison>, String>),
    Done(Result<String, String>),
}

/// Handle to the query thread. Dropping it stops the thread.
pub struct Worker {
    tx: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Reply>,
}

impl Worker {
    pub fn spawn(ui: egui::Context) -> Self {
        let (tx, req_rx) = mpsc::channel::<Request>();
        let (reply_tx, rx) = mpsc::channel::<Reply>();
        std::thread::Builder::new()
            .name("queries".into())
            .spawn(move || serve(req_rx, reply_tx, ui))
            .expect("spawn query thread");
        Self { tx, rx }
    }

    pub fn send(&self, r: Request) {
        let _ = self.tx.send(r);
    }

    pub fn poll(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
}

fn serve(rx: mpsc::Receiver<Request>, tx: mpsc::Sender<Reply>, ui: egui::Context) {
    let mut cache: Option<(PathBuf, Store)> = None;
    while let Ok(first) = rx.recv() {
        // Coalesce: only the newest request of each kind matters, except
        // exports and saves, which all run.
        let mut latest: [Option<Request>; 6] = Default::default();
        let mut jobs = vec![];
        for req in std::iter::once(first).chain(rx.try_iter()) {
            let slot = match req {
                Request::Rows(_) => 0,
                Request::Counts(_) => 1,
                Request::Stats(_) => 2,
                Request::Detail { .. } => 3,
                Request::Diagram(_) => 4,
                Request::Compare { .. } => 5,
                other => {
                    jobs.push(other);
                    continue;
                }
            };
            latest[slot] = Some(req);
        }
        for req in latest.into_iter().flatten().chain(jobs) {
            let reply = handle(&mut cache, req);
            let _ = tx.send(reply);
            ui.request_repaint();
        }
    }
}

fn handle(cache: &mut Option<(PathBuf, Store)>, req: Request) -> Reply {
    match req {
        Request::Rows(v) => Reply::Rows(with_store(cache, &v.db.clone(), |s| load_rows(s, v))),
        Request::Counts(db) => Reply::Counts(with_store(cache, &db, |s| load_counts(s, &db))),
        Request::Stats(db) => Reply::Stats(with_store(cache, &db, |s| load_stats(s, &db))),
        Request::Detail { db, url } => Reply::Detail(with_store(cache, &db, |s| load_detail(s, &url).map(Box::new))),
        Request::Diagram(db) => Reply::Diagram(with_store(cache, &db, |s| load_diagram(s, &db))),
        Request::Compare { db, baseline } => {
            Reply::Compare(with_store(cache, &db, |s| load_comparison(s, &db, &baseline).map(Box::new)))
        }
        Request::Export(job) => Reply::Done(with_store(cache, &job.db.clone(), |s| run_export(s, &job))),
        Request::SaveCopy { db, target } => Reply::Done(with_store(cache, &db, |s| {
            let _ = std::fs::remove_file(&target);
            s.conn().execute("VACUUM INTO ?1", [target.to_string_lossy()])?;
            Ok(format!("Saved a copy to {}", target.display()))
        })),
    }
}

/// Run `f` against the store at `path`, reopening only when the path changes.
fn with_store<T>(
    cache: &mut Option<(PathBuf, Store)>,
    path: &Path,
    f: impl FnOnce(&Store) -> anyhow::Result<T>,
) -> Result<T, String> {
    if cache.as_ref().is_none_or(|(p, _)| p != path) {
        *cache = None;
        // Crawls from older versions lack newer columns: a brief read-write
        // open adds them. Read-only media just skip this step.
        if let Err(e) = Store::open_existing(path) {
            tracing::warn!("could not upgrade {}: {e:#}", path.display());
        }
        let store = Store::open_readonly(path).map_err(|e| format!("{e:#}"))?;
        *cache = Some((path.to_path_buf(), store));
    }
    let (_, store) = cache.as_ref().expect("store opened above");
    f(store).map_err(|e| format!("{e:#}"))
}

fn int(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        _ => None,
    }
}

fn load_rows(store: &Store, view: View) -> anyhow::Result<RowsReply> {
    let t = view.run(store, Some(ROW_LIMIT + 1))?;
    let status_col = VIEWS[view.tab].columns.iter().position(|c| *c == "status_code");
    let truncated = t.rows.len() > ROW_LIMIT;
    let rows = t
        .rows
        .into_iter()
        .take(ROW_LIMIT)
        .map(|r| Row {
            url: value_text(&r[0]),
            status: status_col.and_then(|i| int(&r[i])),
            cells: r.iter().map(value_text).collect(),
        })
        .collect();
    Ok(RowsReply { view, rows, truncated })
}

/// URL counts for every filter. One query per view: its scope narrows the
/// rows once, and its filters are only evaluated on rows in scope.
fn load_counts(store: &Store, db: &Path) -> anyhow::Result<Counts> {
    let total = store.url_count()?;
    let mut filters = Vec::new();
    for view in VIEWS {
        let sums: Vec<String> = view
            .filters
            .iter()
            .map(|f| format!("COALESCE(SUM(CASE WHEN {} THEN 1 ELSE 0 END), 0)", f.sql))
            .collect();
        let sql = format!("SELECT {} FROM urls WHERE {}", sums.join(", "), view.scope);
        let row = store.table(&sql)?.rows.into_iter().next().unwrap_or_default();
        filters.extend((0..view.filters.len()).map(|i| row.get(i).and_then(int).unwrap_or(0) as u64));
    }
    Ok(Counts { db: db.to_path_buf(), filters, total })
}

fn load_stats(store: &Store, db: &Path) -> anyhow::Result<Stats> {
    let mut depth: BTreeMap<u32, u64> = BTreeMap::new();
    for r in store.table("SELECT depth, COUNT(*) FROM urls WHERE internal = 1 AND depth >= 0 GROUP BY depth")?.rows {
        let d = int(&r[0]).unwrap_or(0).clamp(0, 10) as u32;
        *depth.entry(d).or_default() += int(&r[1]).unwrap_or(0) as u64;
    }
    let mut response_times = [0u64; 11];
    for r in store
        .table("SELECT MIN(response_ms / 1000, 10), COUNT(*) FROM urls WHERE internal = 1 AND resource_type = 'html' AND response_ms IS NOT NULL GROUP BY 1")?
        .rows
    {
        let b = int(&r[0]).unwrap_or(0).clamp(0, 10) as usize;
        response_times[b] += int(&r[1]).unwrap_or(0) as u64;
    }
    let paths = store.table("SELECT url_path(url) FROM urls WHERE internal = 1 AND resource_type = 'html'")?;
    let directories = directory_tree(paths.rows.iter().map(|r| value_text(&r[0])));
    let notes = store.get_meta("diagnostics")?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    let sitemap_only = store.table("SELECT COUNT(*) FROM urls WHERE internal = 1 AND depth < 0")?.rows.first().and_then(|r| int(&r[0])).unwrap_or(0) as u64;
    Ok(Stats { db: db.to_path_buf(), depth: depth.into_iter().collect(), response_times, directories, notes, sitemap_only })
}

/// Pages per first and second path segment, largest first.
fn directory_tree(paths: impl Iterator<Item = String>) -> Vec<DirNode> {
    let mut top: HashMap<String, (u64, HashMap<String, u64>)> = HashMap::new();
    for p in paths {
        let path = p.split(['?', '#']).next().unwrap_or("/");
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let first = match segs.first() {
            Some(s) if segs.len() > 1 || path.ends_with('/') => format!("/{s}/"),
            _ => "/".to_string(),
        };
        let entry = top.entry(first.clone()).or_default();
        entry.0 += 1;
        if segs.len() > 2 || (segs.len() == 2 && path.ends_with('/')) {
            *entry.1.entry(format!("{first}{}/", segs[1])).or_default() += 1;
        }
    }
    let mut nodes: Vec<DirNode> = top
        .into_iter()
        .map(|(path, (count, kids))| {
            let mut children: Vec<DirNode> =
                kids.into_iter().map(|(path, count)| DirNode { path, count, children: vec![] }).collect();
            children.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
            children.truncate(25);
            DirNode { path, count, children }
        })
        .collect();
    nodes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
    nodes.truncate(60);
    nodes
}

/// Columns of a URL row, for the detail panel and exports.
const ALL_FIELDS: &str = "url, status, indexability, indexable, resource_type, content_type, \
    title, title_len, title_px, meta_description, meta_description_len, meta_description_px, meta_robots, \
    x_robots_tag, canonical, h1, h1_count, h2, h2_count, lang, word_count, outlinks, \
    COALESCE((SELECT i.total FROM inlinks i WHERE i.url = urls.url), 0) AS inlinks, \
    size, response_ms, depth, internal, redirect_to, in_sitemap, structured_data, robots_blocked, rendered, \
    render_error, error, truncated, dup_hash, crawled_at, meta_refresh, near_duplicates, link_score, found_via,     sitemap_lastmod, js_only_links, js_only_words";

fn load_detail(store: &Store, url: &str) -> anyhow::Result<Detail> {
    let row = store.query(&format!("SELECT {ALL_FIELDS}, headers, html FROM urls WHERE url = ?1"), &[&url])?;
    let values = row.rows.first().cloned().unwrap_or_default();
    let get = |name: &str| row.columns.iter().position(|c| c == name).and_then(|i| values.get(i)).cloned().unwrap_or(Value::Null);
    let fields = FIELDS
        .iter()
        .filter_map(|(col, label, fmt)| Some((label.to_string(), present(&get(col), *fmt)?)))
        .collect();
    let text = |name: &str| match get(name) {
        Value::Text(s) => Some(s),
        _ => None,
    };
    let headers = text("headers")
        .map(|h| {
            h.lines()
                .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
                .collect()
        })
        .unwrap_or_default();
    let serp = Serp {
        title: text("title").unwrap_or_default(),
        description: text("meta_description").unwrap_or_default(),
        title_px: int(&get("title_px")).unwrap_or(0) as u32,
        description_px: int(&get("meta_description_px")).unwrap_or(0) as u32,
    };
    let link_type = "CASE l.kind WHEN 'a' THEN 'Hyperlink' WHEN 'img' THEN 'Image' WHEN 'script' THEN 'JavaScript' \
        WHEN 'css' THEN 'CSS' WHEN 'iframe' THEN 'Iframe' WHEN 'canonical' THEN 'Canonical' WHEN 'hreflang' THEN 'Hreflang' ELSE l.kind END";
    let inlinks = store.query(
        &format!(
            "SELECT l.from_url AS \"From\", {link_type} AS \"Type\", l.anchor AS \"Anchor Text\",
                CASE l.nofollow WHEN 1 THEN 'No' ELSE 'Yes' END AS \"Follow\", u.status AS \"Status Code\"
             FROM links l LEFT JOIN urls u ON u.url = l.from_url
             WHERE l.to_url = ?1 ORDER BY l.from_url LIMIT 10000"
        ),
        &[&url],
    )?;
    let outlinks = store.query(
        &format!(
            "SELECT l.to_url AS \"To\", {link_type} AS \"Type\", l.anchor AS \"Anchor Text\",
                CASE l.nofollow WHEN 1 THEN 'No' ELSE 'Yes' END AS \"Follow\", u.status AS \"Status Code\"
             FROM links l LEFT JOIN urls u ON u.url = l.to_url
             WHERE l.from_url = ?1 ORDER BY l.rowid LIMIT 10000"
        ),
        &[&url],
    )?;
    let images = store.query(
        "SELECT l.to_url AS \"Image\", l.anchor AS \"Alt Text\", u.status AS \"Status Code\",
                ROUND(u.size / 1024.0, 1) AS \"Size (KB)\"
         FROM links l LEFT JOIN urls u ON u.url = l.to_url
         WHERE l.from_url = ?1 AND l.kind = 'img' ORDER BY l.rowid LIMIT 5000",
        &[&url],
    )?;
    let resources = store.query(
        &format!(
            "SELECT l.to_url AS \"Resource\", {link_type} AS \"Type\", u.status AS \"Status Code\",
                ROUND(u.size / 1024.0, 1) AS \"Size (KB)\"
             FROM links l LEFT JOIN urls u ON u.url = l.to_url
             WHERE l.from_url = ?1 AND l.kind IN ('script', 'css', 'iframe') ORDER BY l.rowid LIMIT 5000"
        ),
        &[&url],
    )?;
    let duplicates = store.query(
        "SELECT o.url AS \"Address\",
                CASE WHEN o.dup_hash = u.dup_hash THEN 'Exact duplicate content'
                     WHEN o.title = u.title THEN 'Same title'
                     ELSE 'Same meta description' END AS \"Match\"
         FROM urls u JOIN urls o ON o.url <> u.url AND o.internal = 1 AND o.resource_type = 'html' AND o.status = 200
              AND (o.dup_hash = u.dup_hash OR o.title = u.title OR o.meta_description = u.meta_description)
         WHERE u.url = ?1 AND u.resource_type = 'html' ORDER BY 2, 1 LIMIT 2000",
        &[&url],
    )?;
    Ok(Detail {
        url: url.to_string(),
        fields,
        inlinks,
        outlinks,
        images,
        resources,
        duplicates,
        headers,
        source: text("html"),
        serp,
    })
}

fn url_set(store: &Store, condition: &str) -> anyhow::Result<std::collections::HashSet<String>> {
    Ok(store
        .table(&format!("SELECT url FROM urls WHERE {condition}"))?
        .rows
        .iter()
        .map(|r| value_text(&r[0]))
        .collect())
}

fn load_comparison(store: &Store, db: &Path, baseline: &Path) -> anyhow::Result<Comparison> {
    anyhow::ensure!(baseline != db, "pick a different crawl to compare with");
    // Older crawl files may need their columns upgraded first.
    if let Err(e) = Store::open_existing(baseline) {
        tracing::warn!("could not upgrade {}: {e:#}", baseline.display());
    }
    let old = Store::open_readonly(baseline)?;
    let pages = "internal = 1 AND resource_type = 'html'";
    let (now_pages, old_pages) = (url_set(store, pages)?, url_set(&old, pages)?);
    let crawled_now = url_set(store, "1")?;
    let mut problems = vec![];
    for (vi, fi, _) in model::problems() {
        let cond = model::condition(vi, fi);
        let (now, before) = (url_set(store, &cond)?, url_set(&old, &cond)?);
        let mut new: Vec<String> = now.difference(&before).cloned().collect();
        let mut fixed: Vec<String> = before.difference(&now).filter(|u| crawled_now.contains(*u)).cloned().collect();
        if new.is_empty() && fixed.is_empty() {
            continue;
        }
        new.sort();
        fixed.sort();
        problems.push(ProblemDelta { view: vi, filter: fi, before: before.len() as u64, now: now.len() as u64, new, fixed });
    }
    problems.sort_by_key(|d| std::cmp::Reverse(d.new.len() + d.fixed.len()));
    Ok(Comparison {
        db: db.to_path_buf(),
        baseline: baseline.to_path_buf(),
        pages_added: now_pages.difference(&old_pages).count() as u64,
        pages_removed: old_pages.difference(&now_pages).count() as u64,
        problems,
    })
}

fn load_diagram(store: &Store, db: &Path) -> anyhow::Result<Diagram> {
    let t = store.table(&format!(
        "SELECT url, depth, status, indexable, COALESCE((SELECT i.anchors FROM inlinks i WHERE i.url = urls.url), 0)
         FROM urls WHERE internal = 1 AND resource_type IN ('html', 'redirect')
         ORDER BY depth, id LIMIT {}",
        DIAGRAM_LIMIT + 1
    ))?;
    let truncated = t.rows.len() > DIAGRAM_LIMIT;
    let mut nodes: Vec<DiagramNode> = t
        .rows
        .iter()
        .take(DIAGRAM_LIMIT)
        .map(|r| DiagramNode {
            url: value_text(&r[0]),
            depth: int(&r[1]).unwrap_or(0) as u32,
            status: int(&r[2]),
            indexable: int(&r[3]) == Some(1),
            inlinks: int(&r[4]).unwrap_or(0) as u64,
            parent: None,
        })
        .collect();
    let index: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, n)| (n.url.clone(), i)).collect();
    let mut parent_link = store.conn().prepare(
        "SELECT l.from_url FROM links l JOIN urls p ON p.url = l.from_url
         WHERE l.to_url = ?1 AND l.kind = 'a' AND p.depth = ?2 - 1 ORDER BY p.id LIMIT 1",
    )?;
    let mut parent_redirect = store.conn().prepare("SELECT url FROM urls WHERE redirect_to = ?1 ORDER BY id LIMIT 1")?;
    for (i, node) in nodes.iter_mut().enumerate() {
        if node.depth == 0 {
            continue;
        }
        let from: Option<String> = parent_link
            .query_row(rusqlite::params![node.url, node.depth as i64], |r| r.get(0))
            .ok()
            .or_else(|| parent_redirect.query_row([&node.url], |r| r.get(0)).ok());
        node.parent = from.and_then(|f| index.get(&f).copied()).filter(|p| *p != i);
    }
    Ok(Diagram { db: db.to_path_buf(), nodes, truncated })
}

// ---- exports ---------------------------------------------------------------

/// Static bulk exports and reports.
pub mod sql {
    pub const ALL_INLINKS: &str = "SELECT l.to_url AS \"Destination\", l.from_url AS \"Source\", l.kind AS \"Type\", l.anchor AS \"Anchor\",
        CASE l.nofollow WHEN 1 THEN 'false' ELSE 'true' END AS \"Follow\", u.status AS \"Status Code\"
        FROM links l LEFT JOIN urls u ON u.url = l.to_url ORDER BY l.to_url";
    pub const ALL_OUTLINKS: &str = "SELECT l.from_url AS \"Source\", l.to_url AS \"Destination\", l.kind AS \"Type\", l.anchor AS \"Anchor\",
        CASE l.nofollow WHEN 1 THEN 'false' ELSE 'true' END AS \"Follow\", u.status AS \"Status Code\"
        FROM links l LEFT JOIN urls u ON u.url = l.to_url ORDER BY l.from_url, l.rowid";
    pub const ALL_ANCHOR_TEXT: &str = "SELECT l.from_url AS \"Source\", l.to_url AS \"Destination\", l.anchor AS \"Anchor Text\",
        CASE l.to_internal WHEN 1 THEN 'Internal' ELSE 'External' END AS \"Link\"
        FROM links l WHERE l.kind = 'a' ORDER BY l.to_url";
    macro_rules! inlinks_where {
        ($cond:literal) => {
            concat!(
                "SELECT l.to_url AS \"Destination\", u.status AS \"Status Code\", l.from_url AS \"Source\", l.anchor AS \"Anchor\", l.kind AS \"Type\"
                 FROM links l JOIN urls u ON u.url = l.to_url WHERE ", $cond, " ORDER BY l.to_url, l.from_url"
            )
        };
    }
    pub const REDIRECT_INLINKS: &str = inlinks_where!("u.status BETWEEN 300 AND 399");
    pub const CLIENT_ERROR_INLINKS: &str = inlinks_where!("u.status BETWEEN 400 AND 499");
    pub const SERVER_ERROR_INLINKS: &str = inlinks_where!("u.status >= 500");
    pub const NO_RESPONSE_INLINKS: &str = inlinks_where!("u.status IS NULL AND u.robots_blocked = 0");
    pub const IMAGE_ALT_TEXT: &str = "SELECT l.from_url AS \"Page\", l.to_url AS \"Image\", l.anchor AS \"Alt Text\", u.size AS \"Size (bytes)\"
        FROM links l LEFT JOIN urls u ON u.url = l.to_url WHERE l.kind = 'img' ORDER BY l.from_url";
    pub const IMAGES_MISSING_ALT: &str = "SELECT l.from_url AS \"Page\", l.to_url AS \"Image\", u.size AS \"Size (bytes)\"
        FROM links l LEFT JOIN urls u ON u.url = l.to_url WHERE l.kind = 'img' AND l.anchor = '' ORDER BY l.from_url";
    pub const INSECURE_CONTENT: &str = "SELECT l.from_url AS \"Page\", l.to_url AS \"Insecure Resource\", l.kind AS \"Type\"
        FROM links l WHERE l.from_url LIKE 'https://%' AND l.to_url LIKE 'http://%' AND l.kind IN ('img', 'script', 'css', 'iframe')
        ORDER BY l.from_url";
    pub const ORPHAN_PAGES: &str = "SELECT url AS \"Address\", status AS \"Status Code\", indexability AS \"Indexability\"
        FROM urls WHERE in_sitemap = 1 AND NOT EXISTS (SELECT 1 FROM inlinks i WHERE i.url = urls.url AND i.anchors > 0) ORDER BY url";
    pub const SERP_SUMMARY: &str = "SELECT url AS \"Address\", title AS \"Title\", title_len AS \"Title Length\", title_px AS \"Title Pixel Width\",
        meta_description AS \"Meta Description\", meta_description_len AS \"Description Length\", meta_description_px AS \"Description Pixel Width\"
        FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200 ORDER BY id";
    pub const NON_INDEXABLE_CANONICALS: &str = "SELECT u.url AS \"Address\", u.canonical AS \"Canonical\", c.status AS \"Canonical Status Code\",
        c.indexability AS \"Canonical Indexability\" FROM urls u JOIN urls c ON c.url = u.canonical
        WHERE c.url <> u.url AND c.indexable = 0 ORDER BY u.url";
}

fn run_export(store: &Store, job: &ExportJob) -> anyhow::Result<String> {
    let target = &job.target;
    let written = |what: &str| format!("Exported {what} to {}", target.display());
    match &job.kind {
        ExportKind::View(v) => {
            engine::export::write_csv(&v.run(store, None)?, target)?;
            Ok(written("the view"))
        }
        ExportKind::Sql(sql) => {
            engine::export::export_query(store, sql, target)?;
            Ok(written("the data"))
        }
        ExportKind::CrawlOverview => {
            let counts = load_counts(store, &job.db)?;
            let mut t = Table { columns: vec!["View".into(), "Filter".into(), "URLs".into(), "Share of view".into()], rows: vec![] };
            for (ti, tab) in VIEWS.iter().enumerate() {
                let all = counts.filters[model::flat_index(ti, 0)];
                for (fi, f) in tab.filters.iter().enumerate() {
                    let n = counts.filters[model::flat_index(ti, fi)];
                    t.rows.push(vec![
                        Value::Text(tab.title.into()),
                        Value::Text(f.label.into()),
                        Value::Integer(n as i64),
                        Value::Text(percent(n, all)),
                    ]);
                }
            }
            engine::export::write_csv(&t, target)?;
            Ok(written("the crawl overview"))
        }
        ExportKind::IssuesOverview => {
            let counts = load_counts(store, &job.db)?;
            let mut t = Table {
                columns: ["Problem", "Check", "Severity", "Impact", "URLs", "Share", "What it means", "How to fix"]
                    .map(String::from)
                    .to_vec(),
                rows: vec![],
            };
            for (ti, fi, info) in model::problems() {
                let n = counts.filters[model::flat_index(ti, fi)];
                if n == 0 {
                    continue;
                }
                let all = counts.filters[model::flat_index(ti, 0)];
                t.rows.push(vec![
                    Value::Text(model::problem_name(ti, fi)),
                    Value::Text(VIEWS[ti].filters[fi].slug.into()),
                    Value::Text(info.severity.label().into()),
                    Value::Text(info.impact.label().into()),
                    Value::Integer(n as i64),
                    Value::Text(percent(n, all)),
                    Value::Text(info.about.into()),
                    Value::Text(info.fix.into()),
                ]);
            }
            engine::export::write_csv(&t, target)?;
            Ok(written("the issues overview"))
        }
        ExportKind::AllIssues => {
            std::fs::create_dir_all(target)?;
            let counts = load_counts(store, &job.db)?;
            let mut files = 0;
            for (ti, fi, _) in model::problems() {
                if counts.filters[model::flat_index(ti, fi)] == 0 {
                    continue;
                }
                let v = View { db: job.db.clone(), tab: ti, filter: fi, search: String::new(), sort: None };
                let name = format!("{}.csv", slug(&model::problem_name(ti, fi)));
                engine::export::write_csv(&v.run(store, None)?, &target.join(name))?;
                files += 1;
            }
            Ok(format!("Exported {files} issue files to {}", target.display()))
        }
        ExportKind::AllRedirects | ExportKind::RedirectChains { .. } => {
            let chains = redirect_chains(store, matches!(job.kind, ExportKind::RedirectChains { .. }))?;
            let errors_only = matches!(job.kind, ExportKind::RedirectChains { errors_only: true });
            let mut t = Table {
                columns: ["Address", "Redirects", "Final Address", "Final Status Code", "Loop", "Chain"].map(String::from).to_vec(),
                rows: vec![],
            };
            for c in chains {
                if errors_only && !c.final_status.is_none_or(|s| s >= 400) && !c.is_loop {
                    continue;
                }
                t.rows.push(vec![
                    Value::Text(c.start),
                    Value::Integer(c.redirects as i64),
                    Value::Text(c.final_url),
                    c.final_status.map_or(Value::Null, Value::Integer),
                    Value::Text(if c.is_loop { "Yes" } else { "No" }.into()),
                    Value::Text(c.hops.join(" > ")),
                ]);
            }
            engine::export::write_csv(&t, target)?;
            Ok(written("the redirects report"))
        }
        ExportKind::PageSource => {
            std::fs::create_dir_all(target)?;
            let t = store.table("SELECT url, html FROM urls WHERE html IS NOT NULL ORDER BY id")?;
            anyhow::ensure!(
                !t.rows.is_empty(),
                "this crawl did not keep page HTML. Turn on Configuration > Spider > Advanced > Store HTML and crawl again"
            );
            for r in &t.rows {
                let name = format!("{}.html", slug(&value_text(&r[0])));
                std::fs::write(target.join(name), value_text(&r[1]))?;
            }
            Ok(format!("Saved the source of {} pages to {}", t.rows.len(), target.display()))
        }
        ExportKind::XmlSitemap => {
            let t = store.table("SELECT url FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200 AND indexable = 1 ORDER BY depth, id")?;
            anyhow::ensure!(!t.rows.is_empty(), "the crawl has no indexable pages to list");
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
            for r in t.rows.iter().take(50_000) {
                xml.push_str(&format!("  <url><loc>{}</loc></url>\n", xml_escape(&value_text(&r[0]))));
            }
            xml.push_str("</urlset>\n");
            std::fs::write(target, xml)?;
            let note = if t.rows.len() > 50_000 { " (first 50,000 URLs, the sitemap limit)" } else { "" };
            Ok(format!("Wrote a sitemap of {} pages{note} to {}", t.rows.len().min(50_000), target.display()))
        }
        ExportKind::ImageSitemap => {
            let t = store.table(
                "SELECT l.from_url, l.to_url FROM links l JOIN urls p ON p.url = l.from_url
                 WHERE l.kind = 'img' AND p.indexable = 1 AND p.internal = 1 ORDER BY p.id, l.rowid",
            )?;
            anyhow::ensure!(!t.rows.is_empty(), "no images were found on indexable pages");
            let mut xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" \
                 xmlns:image=\"http://www.google.com/schemas/sitemap-image/1.1\">\n",
            );
            let mut current: Option<String> = None;
            for r in &t.rows {
                let page = value_text(&r[0]);
                if current.as_deref() != Some(page.as_str()) {
                    if current.is_some() {
                        xml.push_str("  </url>\n");
                    }
                    xml.push_str(&format!("  <url>\n    <loc>{}</loc>\n", xml_escape(&page)));
                    current = Some(page);
                }
                xml.push_str(&format!("    <image:image><image:loc>{}</image:loc></image:image>\n", xml_escape(&value_text(&r[1]))));
            }
            xml.push_str("  </url>\n</urlset>\n");
            std::fs::write(target, xml)?;
            Ok(written("the image sitemap"))
        }
    }
}

struct Chain {
    start: String,
    /// Redirect hops followed (a loop counts every hop until it repeats).
    redirects: usize,
    hops: Vec<String>,
    final_url: String,
    final_status: Option<i64>,
    is_loop: bool,
}

/// Follow every redirect to its end. With `chains_only`, only redirects
/// that take two or more hops are kept.
fn redirect_chains(store: &Store, chains_only: bool) -> anyhow::Result<Vec<Chain>> {
    let t = store.table("SELECT url, status, redirect_to FROM urls")?;
    let info: HashMap<String, (Option<i64>, Option<String>)> = t
        .rows
        .iter()
        .map(|r| (value_text(&r[0]), (int(&r[1]), match &r[2] { Value::Text(s) => Some(s.clone()), _ => None })))
        .collect();
    let is_redirect = |url: &String| info.get(url).is_some_and(|(s, to)| s.is_some_and(|s| (300..400).contains(&s)) && to.is_some());
    let targets: std::collections::HashSet<&String> = info.values().filter_map(|(_, to)| to.as_ref()).collect();
    // Chains start at redirects nothing redirects to. Redirects left over
    // after that are on loops, where every URL is someone's target.
    let mut starts: Vec<&String> = info.keys().filter(|u| is_redirect(u) && !targets.contains(u)).collect();
    starts.sort();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = vec![];
    let mut loops: Vec<&String> = info.keys().filter(|u| is_redirect(u)).collect();
    loops.sort();
    for url in starts.into_iter().chain(loops) {
        if seen.contains(url) {
            continue;
        }
        let mut hops = vec![url.clone()];
        let mut current = url.clone();
        let mut is_loop = false;
        while let Some((s, Some(next))) = info.get(&current) {
            if !s.is_some_and(|s| (300..400).contains(&s)) {
                break;
            }
            if hops.contains(next) {
                is_loop = true;
                break;
            }
            hops.push(next.clone());
            current = next.clone();
            if hops.len() > 20 {
                break;
            }
        }
        seen.extend(hops.iter().cloned());
        let redirects = if is_loop { hops.len() } else { hops.len() - 1 };
        if chains_only && redirects < 2 && !is_loop {
            continue;
        }
        let final_status = info.get(&current).and_then(|(s, _)| *s);
        out.push(Chain { start: url.clone(), final_url: current, final_status, is_loop, redirects, hops });
    }
    out.sort_by(|a, b| b.redirects.cmp(&a.redirects).then_with(|| a.start.cmp(&b.start)));
    Ok(out)
}

pub fn percent(n: u64, of: u64) -> String {
    if of == 0 { "0%".into() } else { format!("{:.1}%", n as f64 * 100.0 / of as f64) }
}

pub fn slug(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    let s = s.split('_').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("_");
    s.chars().take(120).collect()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

// ---- detail field presentation ------------------------------------------

#[derive(Clone, Copy)]
enum Format {
    Text,
    YesNo,
    /// Shown only when true: an exceptional condition.
    Flag,
    Bytes,
    Millis,
    Chars,
    Pixels,
    UnixTime,
    /// Clicks from the start URL; negative when no link led there.
    Depth,
}

/// URL Details rows: column, label, format, in display order.
const FIELDS: &[(&str, &str, Format)] = &[
    ("url", "Address", Format::Text),
    ("status", "Status Code", Format::Text),
    ("indexability", "Indexability Status", Format::Text),
    ("resource_type", "Type", Format::Text),
    ("content_type", "Content Type", Format::Text),
    ("redirect_to", "Redirect URL", Format::Text),
    ("meta_refresh", "Meta Refresh", Format::Text),
    ("error", "Error", Format::Text),
    ("title", "Title 1", Format::Text),
    ("title_len", "Title 1 Length", Format::Chars),
    ("title_px", "Title 1 Pixel Width", Format::Pixels),
    ("meta_description", "Meta Description 1", Format::Text),
    ("meta_description_len", "Meta Description 1 Length", Format::Chars),
    ("meta_description_px", "Meta Description 1 Pixel Width", Format::Pixels),
    ("canonical", "Canonical Link Element 1", Format::Text),
    ("meta_robots", "Meta Robots 1", Format::Text),
    ("x_robots_tag", "X-Robots-Tag 1", Format::Text),
    ("h1", "H1-1", Format::Text),
    ("h1_count", "H1 Count", Format::Text),
    ("h2", "H2-1", Format::Text),
    ("h2_count", "H2 Count", Format::Text),
    ("lang", "Language", Format::Text),
    ("word_count", "Word Count", Format::Text),
    ("near_duplicates", "No. Near Duplicates", Format::Text),
    ("inlinks", "Inlinks", Format::Text),
    ("outlinks", "Outlinks", Format::Text),
    ("link_score", "Link Score", Format::Text),
    ("depth", "Crawl Depth", Format::Depth),
    ("found_via", "Found Via", Format::Text),
    ("size", "Size", Format::Bytes),
    ("response_ms", "Response Time", Format::Millis),
    ("internal", "Internal", Format::YesNo),
    ("in_sitemap", "In Sitemap", Format::YesNo),
    ("sitemap_lastmod", "Sitemap Lastmod", Format::Text),
    ("structured_data", "Structured Data", Format::Text),
    ("rendered", "Rendered with JavaScript", Format::YesNo),
    ("js_only_links", "JavaScript-Only Links", Format::Text),
    ("js_only_words", "JavaScript-Only Words", Format::Text),
    ("render_error", "Rendering Error", Format::Text),
    ("robots_blocked", "Blocked by robots.txt", Format::Flag),
    ("truncated", "Cut off at the size limit", Format::Flag),
    ("dup_hash", "Hash", Format::Text),
    ("crawled_at", "Crawl Timestamp", Format::UnixTime),
];

fn present(v: &Value, fmt: Format) -> Option<String> {
    let int = match v {
        Value::Null => return None,
        Value::Integer(i) => Some(*i),
        _ => None,
    };
    Some(match (fmt, int) {
        (Format::YesNo, Some(i)) => if i != 0 { "Yes" } else { "No" }.into(),
        (Format::Flag, Some(0)) => return None,
        (Format::Depth, Some(d)) if d < 0 => "None (found only in the XML sitemap)".into(),
        (Format::Flag, Some(_)) => "Yes".into(),
        (Format::Bytes, Some(b)) if b >= 1024 * 1024 => format!("{:.1} MB ({b} bytes)", b as f64 / (1024.0 * 1024.0)),
        (Format::Bytes, Some(b)) if b >= 1024 => format!("{:.1} KB ({b} bytes)", b as f64 / 1024.0),
        (Format::Bytes, Some(b)) => format!("{b} bytes"),
        (Format::Millis, Some(ms)) => format!("{ms} ms"),
        (Format::Chars, Some(n)) => format!("{n} characters"),
        (Format::Pixels, Some(n)) => format!("{n} px"),
        (Format::UnixTime, Some(t)) => chrono::DateTime::from_timestamp(t, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| t.to_string()),
        _ => value_text(v),
    })
}

/// Whether a column holds numbers (right-aligned).
pub fn is_numeric(id: &str) -> bool {
    COLS.iter().any(|c| c.id == id && c.numeric)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("sm-gui-q-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        drop(Store::open(&path).unwrap());
        let store = Store::open_readonly(&path).unwrap();
        (path, store)
    }

    #[test]
    fn every_filter_and_column_is_valid_sql() {
        let (path, store) = temp_store("sql");
        load_counts(&store, &path).unwrap();
        for (ti, tab) in VIEWS.iter().enumerate() {
            for fi in 0..tab.filters.len() {
                let sort = Some(Sort { column: tab.columns[tab.columns.len() - 1], descending: true });
                let v = View { db: path.clone(), tab: ti, filter: fi, search: "a_%".into(), sort };
                load_rows(&store, v.clone()).unwrap_or_else(|e| panic!("{} / {}: {e}", tab.title, tab.filters[fi].label));
            }
        }
        load_stats(&store, &path).unwrap();
        load_detail(&store, "https://x/").unwrap();
        load_diagram(&store, &path).unwrap();
        for s in [sql::ALL_INLINKS, sql::ALL_OUTLINKS, sql::ALL_ANCHOR_TEXT, sql::REDIRECT_INLINKS, sql::CLIENT_ERROR_INLINKS,
            sql::SERVER_ERROR_INLINKS, sql::NO_RESPONSE_INLINKS, sql::IMAGE_ALT_TEXT, sql::IMAGES_MISSING_ALT,
            sql::INSECURE_CONTENT, sql::ORPHAN_PAGES, sql::SERP_SUMMARY, sql::NON_INDEXABLE_CANONICALS]
        {
            store.table(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        }
        drop(store);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Times every query the UI runs, on a synthetic large crawl.
    /// `cargo test -p gui --release large_crawl_query_times -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark; run manually"]
    fn large_crawl_query_times() {
        use engine::store::{LinkRecord, UrlRecord};
        use std::time::Instant;
        const PAGES: usize = 30_000;
        let dir = std::env::temp_dir().join(format!("sm-bench-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.db");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = Store::open(&path).unwrap();
            let mut batch = vec![];
            for i in 0..PAGES {
                let url = format!("https://example.com/section-{}/page-{i}", i % 40);
                let rec = UrlRecord {
                    url: url.clone(),
                    depth: (i % 9) as u32,
                    internal: true,
                    status: Some(if i % 50 == 0 { 404 } else { 200 }),
                    content_type: Some("text/html".into()),
                    resource_type: "html",
                    size: Some(20_000 + i as u64),
                    response_ms: Some((i % 3000) as u64),
                    indexable: i % 50 != 0,
                    indexability: "Indexable".into(),
                    headers: Some("content-type: text/html".into()),
                    page: Some(engine::parse::PageData {
                        title: Some(format!("Title {}", i % 5000)),
                        meta_description: (i % 3 == 0).then(|| format!("Description {i}")),
                        h1: vec![format!("Heading {i}")],
                        word_count: (i % 900) as u32,
                        dup_hash: (i % 7000) as u64,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let links: Vec<LinkRecord> = (0..20)
                    .map(|k| LinkRecord {
                        to_url: format!("https://example.com/section-{}/page-{}", (i + k) % 40, (i * 7 + k * 13) % PAGES),
                        kind: if k % 5 == 0 { "img" } else { "a" },
                        anchor: if k % 9 == 0 { String::new() } else { format!("link {k}") },
                        nofollow: k == 3,
                        internal: true,
                        flags: 0,
                    })
                    .collect();
                batch.push((rec, links));
                if batch.len() == 1000 {
                    w.record_batch(batch.iter().map(|(r, l)| (r, l.as_slice()))).unwrap();
                    batch.clear();
                }
            }
        }
        let store = Store::open_readonly(&path).unwrap();
        let time = |label: &str, f: &dyn Fn()| {
            let t = Instant::now();
            f();
            println!("{label:>40}: {:>7.0} ms", t.elapsed().as_secs_f64() * 1000.0);
        };
        let mut per: Vec<(f64, String)> = vec![];
        for (ti, tab) in VIEWS.iter().enumerate() {
            for fi in 0..tab.filters.len() {
                let t = Instant::now();
                store.table(&format!("SELECT COUNT(*) FROM urls WHERE {}", model::condition(ti, fi))).unwrap();
                per.push((t.elapsed().as_secs_f64() * 1000.0, model::problem_name(ti, fi)));
            }
        }
        per.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (ms, name) in per.iter().take(12) {
            println!("{name:>60}: {ms:>7.0} ms");
        }
        time("counts (every filter)", &|| {
            load_counts(&store, &path).unwrap();
        });
        time("stats", &|| {
            load_stats(&store, &path).unwrap();
        });
        for (ti, tab) in VIEWS.iter().enumerate() {
            time(&format!("rows: {}", tab.title), &|| {
                load_rows(&store, View { db: path.clone(), tab: ti, filter: 0, search: String::new(), sort: None }).unwrap();
            });
        }
        time("rows: Internal sorted by inlinks", &|| {
            let sort = Some(Sort { column: "inlinks", descending: true });
            load_rows(&store, View { db: path.clone(), tab: 0, filter: 0, search: String::new(), sort }).unwrap();
        });
        time("detail", &|| {
            load_detail(&store, "https://example.com/section-1/page-1").unwrap();
        });
        time("diagram", &|| {
            load_diagram(&store, &path).unwrap();
        });
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every export, report and sitemap writes a sensible file.
    #[test]
    fn every_export_writes_its_file() {
        use engine::store::{LinkRecord, UrlRecord};
        let dir = std::env::temp_dir().join(format!("sm-exports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("crawl.db");
        {
            let mut w = Store::open(&db).unwrap();
            let page = |url: &str, title: &str| UrlRecord {
                url: url.into(),
                internal: true,
                status: Some(200),
                resource_type: "html",
                indexable: true,
                indexability: "Indexable".into(),
                in_sitemap: true,
                html: Some(format!("<title>{title}</title>")),
                page: Some(engine::parse::PageData { title: Some(title.into()), ..Default::default() }),
                ..Default::default()
            };
            let redirect = |url: &str, to: &str| UrlRecord {
                url: url.into(),
                internal: true,
                status: Some(301),
                resource_type: "redirect",
                redirect_to: Some(to.into()),
                indexability: "Redirected".into(),
                ..Default::default()
            };
            let a = |to: &str, kind: &'static str, anchor: &str| LinkRecord { to_url: to.into(), kind, anchor: anchor.into(), nofollow: false, internal: true, flags: 0 };
            w.record(&page("https://s.com/", "Home"), &[a("https://s.com/a", "a", "A"), a("http://s.com/i.png", "img", ""), a("https://s.com/r1", "a", "")]).unwrap();
            w.record(&page("https://s.com/a", "Home"), &[a("https://s.com/missing", "a", "click here")]).unwrap();
            w.record(&UrlRecord { url: "https://s.com/missing".into(), internal: true, status: Some(404), resource_type: "html", indexability: "Client Error".into(), ..Default::default() }, &[]).unwrap();
            // r1 -> r2 -> /a (a chain), and a loop l1 <-> l2.
            w.record(&redirect("https://s.com/r1", "https://s.com/r2"), &[]).unwrap();
            w.record(&redirect("https://s.com/r2", "https://s.com/a"), &[]).unwrap();
            w.record(&redirect("https://s.com/l1", "https://s.com/l2"), &[]).unwrap();
            w.record(&redirect("https://s.com/l2", "https://s.com/l1"), &[]).unwrap();
        }
        let store = Store::open_readonly(&db).unwrap();
        let view = View { db: db.clone(), tab: 0, filter: 0, search: String::new(), sort: None };
        let kinds: Vec<(&str, ExportKind)> = vec![
            ("view.csv", ExportKind::View(view)),
            ("inlinks.csv", ExportKind::Sql(sql::ALL_INLINKS)),
            ("outlinks.csv", ExportKind::Sql(sql::ALL_OUTLINKS)),
            ("anchors.csv", ExportKind::Sql(sql::ALL_ANCHOR_TEXT)),
            ("4xx.csv", ExportKind::Sql(sql::CLIENT_ERROR_INLINKS)),
            ("alt.csv", ExportKind::Sql(sql::IMAGES_MISSING_ALT)),
            ("insecure.csv", ExportKind::Sql(sql::INSECURE_CONTENT)),
            ("serp.csv", ExportKind::Sql(sql::SERP_SUMMARY)),
            ("overview.csv", ExportKind::CrawlOverview),
            ("issues.csv", ExportKind::IssuesOverview),
            ("redirects.csv", ExportKind::AllRedirects),
            ("chains.csv", ExportKind::RedirectChains { errors_only: false }),
            ("sitemap.xml", ExportKind::XmlSitemap),
            ("images.xml", ExportKind::ImageSitemap),
            ("issues", ExportKind::AllIssues),
            ("source", ExportKind::PageSource),
        ];
        for (name, kind) in kinds {
            let target = dir.join(name);
            let job = ExportJob { db: db.clone(), kind, target: target.clone() };
            run_export(&store, &job).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(target.exists(), "{name} was not written");
        }
        let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap();
        assert!(read("4xx.csv").contains("https://s.com/missing"));
        assert!(read("insecure.csv").contains("http://s.com/i.png"));
        let chains = read("chains.csv");
        assert!(chains.contains("https://s.com/r1,2,https://s.com/a,200,No"), "{chains}");
        assert!(chains.contains(",Yes,"), "loop should be reported: {chains}");
        let sm = engine::sitemap::parse(&read("sitemap.xml"));
        assert_eq!(sm.urls, vec!["https://s.com/", "https://s.com/a"]);
        assert!(read("issues.csv").contains("Page Titles: Duplicate"));
        assert!(std::fs::read_dir(dir.join("issues")).unwrap().count() > 2);
        assert_eq!(std::fs::read_dir(dir.join("source")).unwrap().count(), 2);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directories_group_by_segment() {
        let tree = directory_tree(["/", "/blog/", "/blog/a", "/blog/2024/x", "/about"].into_iter().map(String::from));
        let blog = tree.iter().find(|n| n.path == "/blog/").unwrap();
        assert_eq!(blog.count, 3);
        assert_eq!(blog.children[0].path, "/blog/2024/");
        assert_eq!(tree.iter().find(|n| n.path == "/").unwrap().count, 2);
    }

    #[test]
    fn field_formats() {
        assert_eq!(present(&Value::Integer(2048), Format::Bytes).as_deref(), Some("2.0 KB (2048 bytes)"));
        assert_eq!(present(&Value::Integer(0), Format::Flag), None);
        assert_eq!(present(&Value::Integer(1), Format::YesNo).as_deref(), Some("Yes"));
        assert_eq!(present(&Value::Null, Format::Text), None);
    }

    #[test]
    fn like_escaping() {
        let v = View { db: PathBuf::new(), tab: 0, filter: 0, search: " 50%_off ".into(), sort: None };
        assert_eq!(v.like_pattern(), "%50\\%\\_off%");
    }
}
