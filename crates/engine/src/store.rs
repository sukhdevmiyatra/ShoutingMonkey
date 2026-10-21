use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, ToSql, params, types::Value};

use crate::parse::PageData;

/// One crawled (or attempted) URL, ready to persist.
#[derive(Debug, Clone, Default)]
pub struct UrlRecord {
    pub url: String,
    pub depth: u32,
    pub internal: bool,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub resource_type: &'static str,
    pub size: Option<u64>,
    pub response_ms: Option<u64>,
    pub redirect_to: Option<String>,
    pub x_robots_tag: Option<String>,
    /// Response headers, one "name: value" per line.
    pub headers: Option<String>,
    /// Listed in one of the site's XML sitemaps.
    pub in_sitemap: bool,
    /// How the crawl found the URL: "start", "link", "redirect", "canonical",
    /// "hreflang", "resource" or "sitemap".
    pub found_via: &'static str,
    /// When rendered: links and words only present after JavaScript ran.
    pub js_only_links: Option<i64>,
    pub js_only_words: Option<i64>,
    /// `<lastmod>` the sitemap gives for this URL.
    pub sitemap_lastmod: Option<String>,
    pub error: Option<String>,
    pub truncated: bool,
    /// Disallowed by robots.txt (crawled anyway in "ignore but report" mode).
    pub robots_blocked: bool,
    /// The page was rendered in the browser (links/content from the DOM).
    pub rendered: bool,
    /// Why rendering failed (the raw HTML was used instead).
    pub render_error: Option<String>,
    /// Raw HTML, kept only when `store_html` is on.
    pub html: Option<String>,
    pub indexable: bool,
    pub indexability: String,
    pub page: Option<PageData>,
}

#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub to_url: String,
    pub kind: &'static str,
    pub anchor: String,
    pub nofollow: bool,
    pub internal: bool,
    /// `parse::LINK_*` bits.
    pub flags: u8,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS urls (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    depth INTEGER NOT NULL,
    internal INTEGER NOT NULL,
    status INTEGER,
    content_type TEXT,
    resource_type TEXT NOT NULL,
    size INTEGER,
    response_ms INTEGER,
    redirect_to TEXT,
    indexable INTEGER NOT NULL,
    indexability TEXT NOT NULL,
    title TEXT,
    title_len INTEGER,
    meta_description TEXT,
    meta_description_len INTEGER,
    meta_robots TEXT,
    x_robots_tag TEXT,
    canonical TEXT,
    h1 TEXT,
    h1_count INTEGER,
    h2 TEXT,
    h2_count INTEGER,
    lang TEXT,
    word_count INTEGER,
    dup_hash TEXT,
    outlinks INTEGER,
    truncated INTEGER NOT NULL DEFAULT 0,
    robots_blocked INTEGER NOT NULL DEFAULT 0,
    rendered INTEGER NOT NULL DEFAULT 0,
    render_error TEXT,
    headers TEXT,
    in_sitemap INTEGER NOT NULL DEFAULT 0,
    title_px INTEGER,
    meta_description_px INTEGER,
    structured_data TEXT,
    out_internal INTEGER,
    out_external INTEGER,
    out_internal_nofollow INTEGER,
    out_no_anchor INTEGER,
    out_generic_anchor INTEGER,
    hreflang_links INTEGER,
    insecure_resources INTEGER,
    meta_refresh TEXT,
    canonical_relative INTEGER,
    heading_skip INTEGER,
    has_viewport INTEGER,
    has_open_graph INTEGER,
    has_twitter_card INTEGER,
    jsonld_errors INTEGER,
    insecure_forms INTEGER,
    simhash TEXT,
    found_via TEXT,
    js_only_links INTEGER,
    js_only_words INTEGER,
    sitemap_lastmod TEXT,
    near_duplicates INTEGER,
    link_score REAL,
    error TEXT,
    html TEXT,
    crawled_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS links (
    from_url TEXT NOT NULL,
    to_url TEXT NOT NULL,
    kind TEXT NOT NULL,
    anchor TEXT,
    nofollow INTEGER NOT NULL,
    to_internal INTEGER NOT NULL,
    flags INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS links_to ON links(to_url);
CREATE INDEX IF NOT EXISTS links_from ON links(from_url);
CREATE INDEX IF NOT EXISTS urls_dup ON urls(dup_hash);
CREATE INDEX IF NOT EXISTS urls_title ON urls(title);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS inlinks (
    url TEXT PRIMARY KEY,
    total INTEGER NOT NULL,
    anchors INTEGER NOT NULL,
    nofollow_anchors INTEGER NOT NULL,
    images INTEGER NOT NULL DEFAULT 0
);
";

/// Columns added after the first release, applied to older databases.
const MIGRATIONS: &[(&str, &str)] = &[
    ("robots_blocked", "ALTER TABLE urls ADD COLUMN robots_blocked INTEGER NOT NULL DEFAULT 0"),
    ("rendered", "ALTER TABLE urls ADD COLUMN rendered INTEGER NOT NULL DEFAULT 0"),
    ("render_error", "ALTER TABLE urls ADD COLUMN render_error TEXT"),
    ("headers", "ALTER TABLE urls ADD COLUMN headers TEXT"),
    ("in_sitemap", "ALTER TABLE urls ADD COLUMN in_sitemap INTEGER NOT NULL DEFAULT 0"),
    ("title_px", "ALTER TABLE urls ADD COLUMN title_px INTEGER"),
    ("meta_description_px", "ALTER TABLE urls ADD COLUMN meta_description_px INTEGER"),
    ("structured_data", "ALTER TABLE urls ADD COLUMN structured_data TEXT"),
    ("out_internal", "ALTER TABLE urls ADD COLUMN out_internal INTEGER"),
    ("out_external", "ALTER TABLE urls ADD COLUMN out_external INTEGER"),
    ("out_internal_nofollow", "ALTER TABLE urls ADD COLUMN out_internal_nofollow INTEGER"),
    ("out_no_anchor", "ALTER TABLE urls ADD COLUMN out_no_anchor INTEGER"),
    ("out_generic_anchor", "ALTER TABLE urls ADD COLUMN out_generic_anchor INTEGER"),
    ("hreflang_links", "ALTER TABLE urls ADD COLUMN hreflang_links INTEGER"),
    ("insecure_resources", "ALTER TABLE urls ADD COLUMN insecure_resources INTEGER"),
    ("meta_refresh", "ALTER TABLE urls ADD COLUMN meta_refresh TEXT"),
    ("canonical_relative", "ALTER TABLE urls ADD COLUMN canonical_relative INTEGER"),
    ("heading_skip", "ALTER TABLE urls ADD COLUMN heading_skip INTEGER"),
    ("has_viewport", "ALTER TABLE urls ADD COLUMN has_viewport INTEGER"),
    ("has_open_graph", "ALTER TABLE urls ADD COLUMN has_open_graph INTEGER"),
    ("has_twitter_card", "ALTER TABLE urls ADD COLUMN has_twitter_card INTEGER"),
    ("jsonld_errors", "ALTER TABLE urls ADD COLUMN jsonld_errors INTEGER"),
    ("insecure_forms", "ALTER TABLE urls ADD COLUMN insecure_forms INTEGER"),
    ("simhash", "ALTER TABLE urls ADD COLUMN simhash TEXT"),
    ("found_via", "ALTER TABLE urls ADD COLUMN found_via TEXT"),
    ("js_only_links", "ALTER TABLE urls ADD COLUMN js_only_links INTEGER"),
    ("js_only_words", "ALTER TABLE urls ADD COLUMN js_only_words INTEGER"),
    ("sitemap_lastmod", "ALTER TABLE urls ADD COLUMN sitemap_lastmod TEXT"),
    ("near_duplicates", "ALTER TABLE urls ADD COLUMN near_duplicates INTEGER"),
    ("link_score", "ALTER TABLE urls ADD COLUMN link_score REAL"),
];

/// Anchor texts that say nothing about the link's target.
pub const GENERIC_ANCHORS: &[&str] = &[
    "click here", "click", "here", "read more", "more", "learn more", "this", "link", "go", "continue",
    "details", "more info",
];

fn is_generic_anchor(anchor: &str) -> bool {
    let a = anchor.trim().to_lowercase();
    GENERIC_ANCHORS.contains(&a.as_str())
}

/// Meta key set once per-page link statistics and the inlinks table are
/// complete (new crawls keep them current; older ones are backfilled).
const LINK_STATS_KEY: &str = "link_stats_v2";

/// Font sizes search results use for titles and descriptions (Arial).
pub const TITLE_FONT_PX: f32 = 20.0;
pub const DESCRIPTION_FONT_PX: f32 = 14.0;

pub struct Store {
    conn: Connection,
}

/// A query result as column names plus rows of SQLite values.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// Render a SQLite value as display/CSV text (NULL as empty).
pub fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Blob(b) => String::from_utf8_lossy(b).into_owned(),
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn opt_i64(v: Option<u64>) -> Option<i64> {
    v.map(|v| v.min(i64::MAX as u64) as i64)
}

impl Store {
    /// Open (creating if needed) a crawl database.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        register_functions(&conn)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open an existing crawl database without creating one.
    pub fn open_existing(path: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(path.is_file(), "no crawl database at {}", path.display());
        Self::open(path)
    }

    /// Open for reading only: safe alongside a running crawl's writer (WAL).
    pub fn open_readonly(path: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(path.is_file(), "no crawl database at {}", path.display());
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let has_urls: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='urls')",
            [],
            |r| r.get(0),
        )?;
        anyhow::ensure!(has_urls, "{} is not a shoutingmonkey crawl database", path.display());
        register_functions(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let existing: Vec<String> = self
            .conn
            .prepare("SELECT name FROM pragma_table_info('urls')")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for (col, sql) in MIGRATIONS {
            if !existing.iter().any(|c| c == col) {
                self.conn.execute_batch(sql)?;
            }
        }
        let link_cols: Vec<String> = self
            .conn
            .prepare("SELECT name FROM pragma_table_info('links')")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if !link_cols.iter().any(|c| c == "flags") {
            self.conn.execute_batch("ALTER TABLE links ADD COLUMN flags INTEGER NOT NULL DEFAULT 0")?;
        }
        if self.get_meta(LINK_STATS_KEY)?.is_none() {
            self.backfill_link_stats()?;
        }
        Ok(())
    }

    /// Compute link statistics for a crawl recorded before they existed.
    fn backfill_link_stats(&self) -> anyhow::Result<()> {
        let generic = GENERIC_ANCHORS.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(", ");
        let has_images: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('inlinks') WHERE name = 'images')",
            [],
            |r| r.get(0),
        )?;
        let tx = self.conn.unchecked_transaction()?;
        if !has_images {
            tx.execute_batch("ALTER TABLE inlinks ADD COLUMN images INTEGER NOT NULL DEFAULT 0")?;
        }
        tx.execute_batch(&format!(
            "DELETE FROM inlinks;
             INSERT INTO inlinks (url, total, anchors, nofollow_anchors, images)
                 SELECT to_url, COUNT(*), SUM(kind = 'a'), SUM(kind = 'a' AND nofollow = 1), SUM(kind = 'img')
                 FROM links GROUP BY to_url;
             UPDATE urls SET
                 out_internal = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'a' AND l.to_internal = 1),
                 out_external = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'a' AND l.to_internal = 0),
                 out_internal_nofollow = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'a' AND l.to_internal = 1 AND l.nofollow = 1),
                 out_no_anchor = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'a' AND l.to_internal = 1 AND trim(l.anchor) = ''),
                 out_generic_anchor = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'a' AND l.to_internal = 1 AND lower(trim(l.anchor)) IN ({generic})),
                 hreflang_links = (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind = 'hreflang'),
                 insecure_resources = CASE WHEN urls.url LIKE 'https://%' THEN (SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.kind IN ('img', 'script', 'css', 'iframe') AND l.to_url LIKE 'http://%') ELSE 0 END
             WHERE outlinks IS NOT NULL;"
        ))?;
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')", [LINK_STATS_KEY])?;
        tx.commit()?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Persist one URL and its outlinks atomically. Re-recording a URL
    /// replaces its row and outlinks.
    pub fn record(&mut self, r: &UrlRecord, links: &[LinkRecord]) -> anyhow::Result<()> {
        self.record_batch(std::iter::once((r, links)))
    }

    /// Persist many URLs in one transaction (far faster than one each).
    pub fn record_batch<'a>(
        &mut self,
        items: impl IntoIterator<Item = (&'a UrlRecord, &'a [LinkRecord])>,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        for (r, links) in items {
            write_one(&tx, r, links)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn url_count(&self) -> anyhow::Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM urls", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Run any read query and collect it as a `Table`.
    pub fn table(&self, sql: &str) -> anyhow::Result<Table> {
        self.query(sql, &[])
    }

    /// Run a parameterized read query (`?1`, `?2`, ...) as a `Table`.
    pub fn query(&self, sql: &str, args: &[&dyn ToSql]) -> anyhow::Result<Table> {
        let mut st = self.conn.prepare(sql)?;
        let columns: Vec<String> = st.column_names().iter().map(|c| c.to_string()).collect();
        let n = columns.len();
        let rows = st
            .query_map(args, |row| (0..n).map(|i| row.get::<_, Value>(i)).collect())?
            .collect::<Result<Vec<Vec<Value>>, _>>()?;
        Ok(Table { columns, rows })
    }
}

/// SQL helpers for report filters:
/// - `text REGEXP pattern` (Rust regex syntax; NULL text never matches)
/// - `url_path(url)`: the percent-decoded path and query of a URL
fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("regexp", 2, flags, |ctx| {
        let re = ctx.get_or_create_aux(0, |v| -> Result<regex::Regex, Box<dyn std::error::Error + Send + Sync>> {
            Ok(regex::Regex::new(v.as_str()?)?)
        })?;
        let text: Option<String> = ctx.get(1)?;
        Ok(text.is_some_and(|t| re.is_match(&t)))
    })?;
    conn.create_scalar_function("url_path", 1, flags, |ctx| {
        let url: Option<String> = ctx.get(0)?;
        Ok(url.and_then(|u| url::Url::parse(&u).ok()).map(|u| {
            let raw = &u[url::Position::BeforePath..url::Position::AfterQuery];
            percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned()
        }))
    })?;
    // max-age of a Strict-Transport-Security header; NULL when absent.
    conn.create_scalar_function("hsts_max_age", 1, flags, |ctx| {
        let headers: Option<String> = ctx.get(0)?;
        Ok(headers.and_then(|h| {
            let value = header_values(&h, "strict-transport-security").next()?;
            value.split(';').find_map(|d| d.trim().to_ascii_lowercase().strip_prefix("max-age=")?.trim_matches('"').parse::<i64>().ok())
        }))
    })?;
    // How many Set-Cookie headers lack an attribute (secure, httponly, samesite).
    conn.create_scalar_function("cookies_missing", 2, flags, |ctx| {
        let headers: Option<String> = ctx.get(0)?;
        let attr: String = ctx.get(1)?;
        Ok(headers.map_or(0, |h| {
            header_values(&h, "set-cookie")
                .filter(|c| !c.split(';').skip(1).any(|a| a.trim().split('=').next().is_some_and(|n| n.eq_ignore_ascii_case(&attr))))
                .count() as i64
        }))
    })?;
    Ok(())
}

/// Values of a header in the stored "name: value" lines.
fn header_values<'a>(headers: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    headers.lines().filter_map(move |l| {
        let (n, v) = l.split_once(':')?;
        n.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// Outlink figures of one page, stored with it so report filters need not
/// scan the links table.
#[derive(Default)]
struct LinkStats {
    internal: i64,
    external: i64,
    internal_nofollow: i64,
    no_anchor: i64,
    generic_anchor: i64,
    hreflang: i64,
    insecure: i64,
}

fn link_stats(page_url: &str, links: &[LinkRecord]) -> LinkStats {
    let https = page_url.starts_with("https://");
    let mut s = LinkStats::default();
    for l in links {
        match l.kind {
            "a" if l.internal => {
                s.internal += 1;
                s.internal_nofollow += i64::from(l.nofollow);
                if l.anchor.trim().is_empty() {
                    s.no_anchor += 1;
                } else if is_generic_anchor(&l.anchor) {
                    s.generic_anchor += 1;
                }
            }
            "a" => s.external += 1,
            "hreflang" => s.hreflang += 1,
            "img" | "script" | "css" | "iframe" if https && l.to_url.starts_with("http://") => s.insecure += 1,
            _ => {}
        }
    }
    s
}

fn write_one(tx: &rusqlite::Transaction, r: &UrlRecord, links: &[LinkRecord]) -> anyhow::Result<()> {
    let p = r.page.as_ref();
    // Re-recording a URL: take its old links out of the inlink counts.
    tx.prepare_cached(
        "UPDATE inlinks SET total = total - c.n, anchors = anchors - c.a, nofollow_anchors = nofollow_anchors - c.nf,
             images = images - c.i
         FROM (SELECT to_url, COUNT(*) AS n, SUM(kind = 'a') AS a, SUM(kind = 'a' AND nofollow = 1) AS nf, SUM(kind = 'img') AS i
               FROM links WHERE from_url = ?1 GROUP BY to_url) AS c
         WHERE inlinks.url = c.to_url",
    )?
    .execute([&r.url])?;
    tx.prepare_cached("DELETE FROM links WHERE from_url = ?1")?.execute([&r.url])?;
    let stats = p.map(|_| link_stats(&r.url, links));
    let len = |t: &Option<String>| t.as_ref().map(|t| t.chars().count() as i64);
    // Sitemap-only URLs were not reached by links, so they have no depth.
    let depth = if r.found_via == "sitemap" { None } else { Some(r.depth as i64) };
    let row: Vec<(&str, Box<dyn ToSql>)> = vec![
        ("url", Box::new(r.url.clone())),
        ("depth", Box::new(depth.unwrap_or(-1))),
        ("internal", Box::new(r.internal)),
        ("status", Box::new(r.status)),
        ("content_type", Box::new(r.content_type.clone())),
        ("resource_type", Box::new(r.resource_type)),
        ("size", Box::new(opt_i64(r.size))),
        ("response_ms", Box::new(opt_i64(r.response_ms))),
        ("redirect_to", Box::new(r.redirect_to.clone())),
        ("indexable", Box::new(r.indexable)),
        ("indexability", Box::new(r.indexability.clone())),
        ("title", Box::new(p.and_then(|p| p.title.clone()))),
        ("title_len", Box::new(p.and_then(|p| len(&p.title)))),
        ("meta_description", Box::new(p.and_then(|p| p.meta_description.clone()))),
        ("meta_description_len", Box::new(p.and_then(|p| len(&p.meta_description)))),
        ("meta_robots", Box::new(p.and_then(|p| p.meta_robots.clone()))),
        ("x_robots_tag", Box::new(r.x_robots_tag.clone())),
        ("canonical", Box::new(p.and_then(|p| p.canonical.clone()))),
        ("h1", Box::new(p.and_then(|p| p.h1.first().cloned()))),
        ("h1_count", Box::new(p.map(|p| p.h1.len() as i64))),
        ("h2", Box::new(p.and_then(|p| p.h2.first().cloned()))),
        ("h2_count", Box::new(p.map(|p| p.h2.len() as i64))),
        ("lang", Box::new(p.and_then(|p| p.lang.clone()))),
        ("word_count", Box::new(p.map(|p| p.word_count))),
        ("dup_hash", Box::new(p.map(|p| format!("{:016x}", p.dup_hash)))),
        ("outlinks", Box::new(p.map(|_| links.len() as i64))),
        ("truncated", Box::new(r.truncated)),
        ("robots_blocked", Box::new(r.robots_blocked)),
        ("rendered", Box::new(r.rendered)),
        ("render_error", Box::new(r.render_error.clone())),
        ("error", Box::new(r.error.clone())),
        ("html", Box::new(r.html.clone())),
        ("crawled_at", Box::new(now_secs())),
        ("headers", Box::new(r.headers.clone())),
        ("in_sitemap", Box::new(r.in_sitemap)),
        ("title_px", Box::new(p.and_then(|p| p.title.as_deref().map(|t| crate::parse::pixel_width(t, TITLE_FONT_PX))))),
        ("meta_description_px", Box::new(p.and_then(|p| p.meta_description.as_deref().map(|t| crate::parse::pixel_width(t, DESCRIPTION_FONT_PX))))),
        ("structured_data", Box::new(p.and_then(|p| p.structured_data.clone()))),
        ("out_internal", Box::new(stats.as_ref().map(|s| s.internal))),
        ("out_external", Box::new(stats.as_ref().map(|s| s.external))),
        ("out_internal_nofollow", Box::new(stats.as_ref().map(|s| s.internal_nofollow))),
        ("out_no_anchor", Box::new(stats.as_ref().map(|s| s.no_anchor))),
        ("out_generic_anchor", Box::new(stats.as_ref().map(|s| s.generic_anchor))),
        ("hreflang_links", Box::new(stats.as_ref().map(|s| s.hreflang))),
        ("insecure_resources", Box::new(stats.as_ref().map(|s| s.insecure))),
        ("meta_refresh", Box::new(p.and_then(|p| p.meta_refresh.clone()))),
        ("canonical_relative", Box::new(p.map(|p| p.canonical_relative))),
        ("heading_skip", Box::new(p.map(|p| p.heading_skip))),
        ("has_viewport", Box::new(p.map(|p| p.has_viewport))),
        ("has_open_graph", Box::new(p.map(|p| p.has_open_graph))),
        ("has_twitter_card", Box::new(p.map(|p| p.has_twitter_card))),
        ("jsonld_errors", Box::new(p.map(|p| p.jsonld_errors))),
        ("insecure_forms", Box::new(p.map(|p| p.insecure_forms))),
        ("simhash", Box::new(p.map(|p| format!("{:016x}", p.content_simhash)))),
        ("found_via", Box::new(r.found_via)),
        ("js_only_links", Box::new(r.js_only_links)),
        ("js_only_words", Box::new(r.js_only_words)),
        ("sitemap_lastmod", Box::new(r.sitemap_lastmod.clone())),
    ];
    let columns: Vec<&str> = row.iter().map(|(c, _)| *c).collect();
    let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
    let sql = format!("INSERT OR REPLACE INTO urls ({}) VALUES ({})", columns.join(", "), marks.join(", "));
    let values: Vec<&dyn ToSql> = row.iter().map(|(_, v)| v.as_ref()).collect();
    tx.prepare_cached(&sql)?.execute(values.as_slice())?;
    let mut st = tx.prepare_cached(
        "INSERT INTO links (from_url, to_url, kind, anchor, nofollow, to_internal, flags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut count = tx.prepare_cached(
        "INSERT INTO inlinks (url, total, anchors, nofollow_anchors, images) VALUES (?1, 1, ?2, ?3, ?4)
         ON CONFLICT(url) DO UPDATE SET total = total + 1, anchors = anchors + excluded.anchors,
             nofollow_anchors = nofollow_anchors + excluded.nofollow_anchors, images = images + excluded.images",
    )?;
    for l in links {
        st.execute(params![r.url, l.to_url, l.kind, l.anchor, l.nofollow, l.internal, l.flags])?;
        let anchor = l.kind == "a";
        count.execute(params![l.to_url, anchor, anchor && l.nofollow, l.kind == "img"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sm-store-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn record_and_rerecord() {
        let path = temp_db("rerecord");
        let mut s = Store::open(&path).unwrap();
        let rec = UrlRecord {
            url: "https://a/".into(),
            resource_type: "html",
            status: Some(200),
            indexable: true,
            indexability: "Indexable".into(),
            page: Some(PageData { title: Some("T".into()), ..Default::default() }),
            ..Default::default()
        };
        let link = LinkRecord {
            to_url: "https://a/b".into(),
            kind: "a",
            anchor: "b".into(),
            nofollow: false,
            internal: true,
            flags: 0,
        };
        s.record(&rec, std::slice::from_ref(&link)).unwrap();
        s.record(&rec, &[link]).unwrap();
        assert_eq!(s.url_count().unwrap(), 1);
        let t = s.table("SELECT COUNT(*) FROM links").unwrap();
        assert_eq!(t.rows[0][0], Value::Integer(1));
        s.set_meta("k", "v").unwrap();
        assert_eq!(s.get_meta("k").unwrap().as_deref(), Some("v"));

        let t = s.query("SELECT url FROM urls WHERE url LIKE ?1", &[&"%a%"]).unwrap();
        assert_eq!(t.rows.len(), 1);
        drop(s);
        let ro = Store::open_readonly(&path).unwrap();
        assert_eq!(ro.url_count().unwrap(), 1);
        drop(ro);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn link_stats_and_inlinks_stay_consistent() {
        let path = temp_db("linkstats");
        let mut s = Store::open(&path).unwrap();
        let link = |to: &str, kind: &'static str, anchor: &str, nofollow: bool, internal: bool| LinkRecord {
            to_url: to.into(),
            kind,
            anchor: anchor.into(),
            nofollow,
            internal,
            flags: 0,
        };
        let page = |url: &str| UrlRecord {
            url: url.into(),
            resource_type: "html",
            page: Some(PageData::default()),
            ..Default::default()
        };
        let links = vec![
            link("https://a/x", "a", "Read more", false, true),
            link("https://a/x", "a", "", true, true),
            link("https://b/", "a", "Other site", false, false),
            link("http://a/i.png", "img", "", false, true),
        ];
        s.record(&page("https://a/"), &links).unwrap();
        s.record(&page("https://a/"), &links).unwrap(); // re-record: counts must not double
        s.record(&page("https://a/y"), &links[..1]).unwrap();
        let row = s
            .table("SELECT out_internal, out_external, out_internal_nofollow, out_no_anchor, out_generic_anchor, insecure_resources FROM urls WHERE url = 'https://a/'")
            .unwrap();
        assert_eq!(row.rows[0], [2, 1, 1, 1, 1, 1].map(Value::Integer).to_vec());
        let inl = s.table("SELECT total, anchors, nofollow_anchors FROM inlinks WHERE url = 'https://a/x'").unwrap();
        assert_eq!(inl.rows[0], [3, 3, 1].map(Value::Integer).to_vec());
        let img = s.table("SELECT images FROM inlinks WHERE url = 'http://a/i.png'").unwrap();
        assert_eq!(img.rows[0][0], Value::Integer(1));

        // A backfill computes the same figures from the links table.
        s.conn().execute_batch("DELETE FROM meta; UPDATE urls SET out_internal = NULL; DELETE FROM inlinks;").unwrap();
        drop(s);
        let s = Store::open(&path).unwrap();
        let t = s.table("SELECT out_internal FROM urls WHERE url = 'https://a/'").unwrap();
        assert_eq!(t.rows[0][0], Value::Integer(2));
        let inl = s.table("SELECT total, anchors, nofollow_anchors FROM inlinks WHERE url = 'https://a/x'").unwrap();
        assert_eq!(inl.rows[0], [3, 3, 1].map(Value::Integer).to_vec());
        drop(s);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn sql_functions() {
        let path = temp_db("functions");
        let s = Store::open(&path).unwrap();
        let t = s
            .table("SELECT url_path('https://a.com/Caf%C3%A9/x_y?q=1'), 'abc' REGEXP 'b', NULL REGEXP 'b', url_path('nope')")
            .unwrap();
        assert_eq!(t.rows[0][0], Value::Text("/Café/x_y?q=1".into()));
        assert_eq!(t.rows[0][1], Value::Integer(1));
        assert_eq!(t.rows[0][2], Value::Integer(0));
        assert_eq!(t.rows[0][3], Value::Null);
        drop(s);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn migrates_old_schema() {
        let path = temp_db("migrate");
        {
            let c = Connection::open(&path).unwrap();
            let old = SCHEMA
                .replace("    robots_blocked INTEGER NOT NULL DEFAULT 0,\n", "")
                .replace("    rendered INTEGER NOT NULL DEFAULT 0,\n", "")
                .replace("    render_error TEXT,\n", "")
                .replace("    headers TEXT,\n", "")
                .replace("    in_sitemap INTEGER NOT NULL DEFAULT 0,\n", "")
                .replace("    title_px INTEGER,\n", "")
                .replace("    meta_description_px INTEGER,\n", "")
                .replace("    structured_data TEXT,\n", "")
                .replace("    out_internal INTEGER,\n", "")
                .replace("    out_external INTEGER,\n", "")
                .replace("    out_internal_nofollow INTEGER,\n", "")
                .replace("    out_no_anchor INTEGER,\n", "")
                .replace("    out_generic_anchor INTEGER,\n", "")
                .replace("    hreflang_links INTEGER,\n", "")
                .replace("    insecure_resources INTEGER,\n", "")
                .replace("    meta_refresh TEXT,\n", "")
                .replace("    canonical_relative INTEGER,\n", "")
                .replace("    heading_skip INTEGER,\n", "")
                .replace("    has_viewport INTEGER,\n", "")
                .replace("    has_open_graph INTEGER,\n", "")
                .replace("    has_twitter_card INTEGER,\n", "")
                .replace("    jsonld_errors INTEGER,\n", "")
                .replace("    insecure_forms INTEGER,\n", "")
                .replace("    simhash TEXT,\n", "")
                .replace("    found_via TEXT,\n", "")
                .replace("    js_only_links INTEGER,\n", "")
                .replace("    js_only_words INTEGER,\n", "")
                .replace("    sitemap_lastmod TEXT,\n", "")
                .replace("    near_duplicates INTEGER,\n", "")
                .replace("    link_score REAL,\n", "");
            c.execute_batch(&old).unwrap();
        }
        let mut s = Store::open(&path).unwrap();
        let rec = UrlRecord { url: "https://a/".into(), resource_type: "html", robots_blocked: true, ..Default::default() };
        s.record(&rec, &[]).unwrap();
        let t = s.table("SELECT robots_blocked FROM urls").unwrap();
        assert_eq!(t.rows[0][0], Value::Integer(1));
        drop(s);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
