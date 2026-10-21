//! Crawls a local site built to be wrong in as many ways as possible and
//! checks that every problem it plants is reported. Runs offline: the site
//! is served by a small HTTP server inside the test.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use engine::checks::{self, VIEWS};
use engine::config::CrawlConfig;
use engine::crawl::{self, CrawlControl};
use engine::store::{Store, value_text};

/// Checks this fixture cannot trigger, with the reason.
const NOT_COVERED: &[(&str, &str)] = &[
    ("security.mixed_content", "needs HTTPS"),
    ("security.insecure_form", "needs HTTPS"),
    ("security.hsts_missing", "needs HTTPS"),
    ("security.hsts_short", "needs HTTPS"),
    ("security.csp_missing", "needs HTTPS"),
    ("security.nosniff", "needs HTTPS"),
    ("security.framing", "needs HTTPS"),
    ("security.referrer", "needs HTTPS"),
    ("security.permissions", "needs HTTPS"),
    ("security.cookie_secure", "needs HTTPS"),
    ("javascript.failed", "needs a browser"),
    ("javascript.links", "needs a browser"),
    ("javascript.content", "needs a browser"),
    ("links.high_external", "needs 100+ external links"),
    ("links.high_internal", "needs 1,000+ internal links"),
    ("performance.very_slow", "needs a 3 second response"),
    ("performance.large_html", "needs a 1 MB page"),
];

struct Page {
    status: u16,
    content_type: &'static str,
    location: Option<String>,
    body: Vec<u8>,
}

fn html(title: Option<&str>, head: &str, body: &str) -> Page {
    let t = title.map(|t| format!("<title>{t}</title>")).unwrap_or_default();
    Page {
        status: 200,
        content_type: "text/html; charset=utf-8",
        location: None,
        body: format!("<!doctype html><html lang=\"en\"><head>{t}{head}</head><body>{body}</body></html>").into_bytes(),
    }
}

fn status(code: u16) -> Page {
    Page { status: code, content_type: "text/html", location: None, body: b"<html><body>error</body></html>".to_vec() }
}

fn redirect(code: u16, to: &str) -> Page {
    Page { status: code, content_type: "text/html", location: Some(to.into()), body: vec![] }
}

const LOREM: &str = "Crawlers read this paragraph to count words on the page so that it does not look thin. ";

fn words(n: usize) -> String {
    LOREM.repeat(n / 16 + 1)
}

/// How the fixture answers robots.txt and the sitemap.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// A robots.txt with a rule and a sitemap.
    Normal,
    /// Neither exists (both 404).
    Bare,
    /// robots.txt fails with 500.
    RobotsError,
}

fn route(path: &str, base: &str, mode: Mode) -> Option<Page> {
    match (mode, path) {
        (Mode::Bare, "/robots.txt" | "/sitemap.xml") => return Some(status(404)),
        (Mode::RobotsError, "/robots.txt") => return Some(status(500)),
        _ => {}
    }
    let long_title = "An unusually long page title that keeps going well past any limit a search engine shows";
    let long_desc = "A meta description that rambles on far past the length search results display, repeating itself, \
                     adding clauses, and generally refusing to stop where a sensible summary would have ended long ago.";
    let long_alt = "An image whose alternative text goes on and on describing every last detail of the picture, the colours, the frame and the wall";
    let deep_url = format!("/{}", "segment/".repeat(16));
    Some(match path {
        "/robots.txt" => Page {
            status: 200,
            content_type: "text/plain",
            location: None,
            body: format!("User-agent: *\nDisallow: /private\nSitemap: {base}/sitemap.xml\n").into_bytes(),
        },
        "/sitemap.xml" => Page {
            status: 200,
            content_type: "application/xml",
            location: None,
            body: format!(
                "<?xml version=\"1.0\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\
                 <url><loc>{base}/</loc><lastmod>2999-01-01</lastmod></url><url><loc>{base}/orphan</loc><lastmod>yesterday</lastmod></url>\
                 <url><loc>{base}/noindex</loc></url><url><loc>{base}/redirect-1</loc></url><url><loc>{base}/missing</loc></url></urlset>"
            )
            .into_bytes(),
        },
        "/" => html(
            Some("Home"),
            "",
            &format!(
                "<h1>Welcome</h1><p>{}</p>
                 <a href=\"/about\">About us</a> <a href=\"/missing\">Gone</a> <a href=\"/redirect-1\">Old link</a>
                 <a href=\"/dup-a\">A</a> <a href=\"/dup-b\">B</a> <a href=\"/thin\">Thin</a> <a href=\"/noindex\">Hidden</a>
                 <a href=\"/canonical-other\">Elsewhere</a> <a href=\"/canonical-bad\">Bad canonical</a>
                 <a href=\"/server-error\">Broken server</a> <a href=\"/soft-404\">Soft</a> <a href=\"/Products/Item_One\">Product</a>
                 <a href=\"/search?q=shoes\">Search</a> <a href=\"/page?utm_source=news\">Campaign</a> <a href=\"/dead-end\">Dead end</a>
                 <a href=\"/nofollow-only\" rel=\"nofollow\">Secret</a> <a href=\"/hreflang\">Languages</a> <a href=\"/structured\">Schema</a>
                 <a href=\"/private/area\">Private</a> <a href=\"/a//b\">Slashes</a> <a href=\"/space%20page\">Space</a>
                 <a href=\"/caf%C3%A9\">Cafe</a> <a href=\"{deep_url}\">Long</a> <a href=\"/deep/1\">Deep</a> <a href=\"/hang\">Slow</a>
                 <a href=\"/about\">click here</a> <a href=\"/about\"></a> <a href=\"http://localhost:1/gone\">Partner</a>
                 <img src=\"/img/photo.png\"> <img src=\"/img/missing.png\" alt=\"Missing\"> <img src=\"/img/big.png\" alt=\"Big\">
                 <img src=\"/img/photo.png\" alt=\"{long_alt}\"> <a href=\"/refresh\">Refresh</a> <a href=\"/blog/\">Blog</a>
                 <a href=\"/headers\">Headers</a> <a href=\"/slow\">Slow</a> <a href=\"/headings\">Headings</a>
                 <a href=\"/no-lang\">No language</a> <a href=\"/bad-json\">Bad JSON</a> <a href=\"/canonical-chain\">Chain</a>",
                words(220)
            ),
        ),
        "/refresh" => html(Some("A page that refreshes to another one"), "<meta http-equiv=\"refresh\" content=\"0; url=/refreshed\">", "<p>Moving</p>"),
        "/refreshed" => html(Some("Where the meta refresh leads to"), "", "<h1>Arrived</h1><a href=\"/\">Home</a>"),
        "/canonical-target" => html(Some("A page only reached through a canonical tag"), "", "<h1>Canonical target</h1>"),
        "/blog/" => html(Some("The blog index page, inside a folder"), "", "<h1>Blog</h1><a href=\"/blog/post\">Post</a> <a href=\"/about\">About (outside the folder)</a>"),
        "/blog/post" => html(Some("A blog post inside the folder"), "<link rel=\"canonical\" href=\"/blog/post\">", "<h1>Post</h1><a href=\"/blog/\">Blog</a>"),
        "/about" => html(
            Some(long_title),
            &format!("<meta name=\"description\" content=\"{long_desc}\"><link rel=\"canonical\" href=\"{base}/\">"),
            &format!("<h1>{long_title} and an even longer heading</h1><h1>Second heading</h1><h2>{long_title} as a subheading</h2><p>{}</p><a href=\"/\">Home</a>", words(220)),
        ),
        "/dup-a" | "/dup-b" => html(
            Some("Duplicate page"),
            "<meta name=\"description\" content=\"The same short description on two pages, written to be shared by both.\">",
            &format!("<h1>Duplicate page</h1><h2>Shared subheading</h2><p>{}</p><a href=\"/\">Home</a>", words(220)),
        ),
        "/thin" => html(None, &format!("<meta name=\"description\" content=\"Short.\"><link rel=\"canonical\" href=\"{base}/canonical-target\">"), "<p>Very little here.</p><a href=\"/\">Home</a>"),
        "/noindex" => html(Some("Hidden page that should not be indexed"), "<meta name=\"robots\" content=\"noindex, nofollow\">", "<h1>Hidden</h1><a href=\"/\">Home</a>"),
        "/canonical-other" => html(Some("Canonical pointing at another site"), "<link rel=\"canonical\" href=\"https://other.example/\">", "<h1>Elsewhere</h1><a href=\"/\">Home</a>"),
        "/canonical-bad" => html(Some("Canonical pointing at a redirect"), &format!("<link rel=\"canonical\" href=\"{base}/redirect-1\">"), "<h1>Bad</h1><a href=\"/\">Home</a>"),
        "/soft-404" => html(Some("Page not found"), "", "<h1>Sorry, this page doesn't exist</h1><a href=\"/\">Home</a>"),
        "/Products/Item_One" | "/search" | "/page" | "/space%20page" | "/caf%C3%A9" | "/a//b" => {
            html(Some("A page reached through an awkward address"), "", "<h1>Awkward address</h1><a href=\"/\">Home</a>")
        }
        "/dead-end" => html(Some("A page with no way out of it at all"), "", "<h1>Dead end</h1><p>Nothing to click.</p>"),
        "/nofollow-only" => html(Some("Reached only through a nofollow link"), "", "<h1>Secret</h1><a href=\"/\">Home</a>"),
        "/hreflang" => html(
            Some("A page with language alternatives"),
            &format!("<link rel=\"alternate\" hreflang=\"de\" href=\"{base}/de/\"><link rel=\"alternate\" hreflang=\"english\" href=\"{base}/en/\">"),
            "<h1>Languages</h1><a href=\"/\">Home</a>",
        ),
        "/structured" => html(
            Some("A page with structured data on it"),
            "<script type=\"application/ld+json\">{\"@context\":\"https://schema.org\",\"@type\":\"Organization\"}</script>",
            "<h1>Schema</h1><a href=\"/\">Home</a>",
        ),
        "/headers" | "/slow" => html(Some("A page whose response headers need work"), "", "<h1>Headers</h1><a href=\"/\">Home</a>"),
        "/headings" => html(Some("A page whose headings skip a level"), "", "<h1></h1><h2>Section</h2><h4>Skipped a level</h4><a href=\"/\">Home</a>"),
        "/no-lang" => Page {
            status: 200,
            content_type: "text/html",
            location: None,
            body: b"<html><head><title>A page that does not say its language</title></head><body><h1>No lang</h1><a href=\"/\">Home</a></body></html>".to_vec(),
        },
        "/bad-json" => html(
            Some("A page with broken structured data"),
            "<script type=\"application/ld+json\">{\"@type\": \"Organization\",}</script>",
            "<h1>Bad JSON</h1><a href=\"/\">Home</a>",
        ),
        "/canonical-chain" => html(Some("Canonical pointing at another canonicalised page"), &format!("<link rel=\"canonical\" href=\"{base}/thin\">"), "<h1>Chain</h1><a href=\"/\">Home</a>"),
        "/de/" => html(Some("German version that does not link back"), "", "<h1>Deutsch</h1><a href=\"/\">Home</a>"),
        "/orphan" => html(Some("Only listed in the sitemap, linked from nowhere"), "", "<h1>Orphan</h1>"),
        "/server-error" => status(500),
        "/missing" | "/img/missing.png" => status(404),
        "/redirect-1" => redirect(301, "/redirect-2"),
        "/redirect-2" => redirect(302, "/about"),
        "/img/photo.png" => Page { status: 200, content_type: "image/png", location: None, body: vec![0u8; 2000] },
        "/img/big.png" => Page { status: 200, content_type: "image/png", location: None, body: vec![0u8; 150_000] },
        p if p.starts_with("/deep/") => {
            let n: u32 = p.trim_start_matches("/deep/").parse().unwrap_or(1);
            // Eight levels: deep enough to trip the depth check, finite like a real site.
            let next = if n < 8 { format!("<a href=\"/deep/{}\">Deeper</a>", n + 1) } else { String::new() };
            html(Some("One more step down a very deep chain of pages"), "", &format!("<h1>Level {n}</h1>{next}"))
        }
        p if p.starts_with("/segment/") => html(Some("A page with a very long address indeed"), "", "<h1>Long</h1><a href=\"/\">Home</a>"),
        _ => return None,
    })
}

fn serve(mut stream: TcpStream, base: String, mode: Mode) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drain the headers.
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok() && line.trim() != "" {
        line.clear();
    }
    let target = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let path = target.split('?').next().unwrap_or("/");
    if path == "/hang" {
        std::thread::sleep(Duration::from_secs(5)); // longer than the crawl's timeout
        return;
    }
    if path == "/slow" {
        std::thread::sleep(Duration::from_millis(1200));
    }
    let page = route(path, &base, mode).unwrap_or_else(|| status(404));
    let mut head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        page.status,
        page.content_type,
        page.body.len()
    );
    if let Some(l) = page.location {
        head.push_str(&format!("Location: {l}\r\n"));
    }
    if path == "/headers" {
        head.push_str("Server: Apache/2.4.1\r\nContent-Security-Policy: script-src 'self' 'unsafe-inline'\r\nSet-Cookie: session=1; Path=/\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&page.body);
}

fn start_server(mode: Mode) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let b = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let b = b.clone();
            std::thread::spawn(move || serve(stream, b, mode));
        }
    });
    base
}

fn test_config() -> CrawlConfig {
    CrawlConfig { timeout_secs: 3, retries: 0, follow_internal_nofollow: true, max_urls: 500, ..Default::default() }
}

/// Crawl the fixture from `start` and open the result.
async fn crawl(mode: Mode, start: &str, name: &str, cfg: CrawlConfig) -> (Store, std::path::PathBuf) {
    let base = start_server(mode);
    let dir = std::env::temp_dir().join(format!("sm-fixture-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("crawl.db");
    crawl::run(cfg, db.clone(), vec![format!("{base}{start}")], None, CrawlControl::new()).await.unwrap();
    (Store::open_readonly(&db).unwrap(), dir)
}

fn one(store: &Store, sql: &str) -> String {
    store.table(sql).unwrap().rows.first().map(|r| value_text(&r[0])).unwrap_or_default()
}

fn diagnostics(store: &Store) -> String {
    store.get_meta("diagnostics").unwrap().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_planted_problem_is_found() {
    let (store, dir) = crawl(Mode::Normal, "/", "all", test_config()).await;
    let crawled: u64 = one(&store, "SELECT COUNT(*) FROM urls").parse().unwrap();
    assert!(crawled > 30, "only {crawled} URLs crawled");

    let mut missing = vec![];
    let mut fired = 0;
    for (vi, fi, _) in checks::problems() {
        let slug = VIEWS[vi].filters[fi].slug;
        if NOT_COVERED.iter().any(|(s, _)| *s == slug) {
            continue;
        }
        if one(&store, &format!("SELECT COUNT(*) FROM urls WHERE {}", checks::condition(vi, fi))) == "0" {
            missing.push(slug);
        } else {
            fired += 1;
        }
    }
    println!("{fired} checks fired on the broken site");
    assert!(missing.is_empty(), "these checks did not fire: {missing:?}");

    // Findings carry stable ids and are ordered worst first.
    let findings = checks::findings(&store, checks::Severity::Notice).unwrap();
    assert!(findings.first().is_some_and(|f| f.severity == "Error"));
    assert!(findings.iter().any(|f| f.check == "response_codes.client_error" && f.url.ends_with("/missing")));

    // How URLs were found: links keep true depths, sitemap-only pages come
    // last with no depth, and canonical and meta refresh targets are crawled.
    assert_eq!(one(&store, "SELECT found_via || ' ' || depth FROM urls WHERE url LIKE '%/orphan'"), "sitemap -1");
    assert_eq!(one(&store, "SELECT found_via FROM urls WHERE url LIKE '%/canonical-target'"), "canonical");
    assert_eq!(one(&store, "SELECT found_via FROM urls WHERE url LIKE '%/refreshed'"), "meta refresh");
    assert_eq!(one(&store, "SELECT depth FROM urls WHERE url LIKE '%/about'"), "1", "linked from the start page");
    let d = diagnostics(&store);
    assert!(d.contains("robots.txt for") && d.contains("found"), "{d}");
    assert!(d.contains("sitemap.xml: 5 URLs"), "{d}");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// No robots.txt and no sitemap is normal for small sites: everything is
/// crawled, and the summary says what was missing.
#[tokio::test(flavor = "multi_thread")]
async fn crawls_without_robots_or_sitemap() {
    let (store, dir) = crawl(Mode::Bare, "/", "bare", test_config()).await;
    let pages: u64 = one(&store, "SELECT COUNT(*) FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200").parse().unwrap();
    assert!(pages > 25, "only {pages} pages crawled without robots.txt");
    assert_eq!(one(&store, "SELECT COUNT(*) FROM urls WHERE robots_blocked = 1"), "0", "nothing is blocked without robots.txt");
    let d = diagnostics(&store);
    assert!(d.contains("not found (404); everything may be crawled"), "{d}");
    assert!(d.contains("answered 404"), "{d}");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A robots.txt that errors does not stop the audit.
#[tokio::test(flavor = "multi_thread")]
async fn robots_server_error_does_not_block() {
    let (store, dir) = crawl(Mode::RobotsError, "/", "robots500", test_config()).await;
    let pages: u64 = one(&store, "SELECT COUNT(*) FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200").parse().unwrap();
    assert!(pages > 25, "only {pages} pages crawled when robots.txt errors");
    assert!(diagnostics(&store).contains("server error 500"));
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Starting inside a folder keeps the crawl there; links outside it are
/// checked but not followed.
#[tokio::test(flavor = "multi_thread")]
async fn stays_inside_the_start_folder() {
    let cfg = CrawlConfig { follow_sitemaps: false, ..test_config() };
    let (store, dir) = crawl(Mode::Normal, "/blog/", "folder", cfg).await;
    let urls: Vec<String> = store.table("SELECT url FROM urls ORDER BY url").unwrap().rows.iter().map(|r| value_text(&r[0])).collect();
    assert!(urls.iter().any(|u| u.ends_with("/blog/post")), "{urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/about")), "the outside link is checked: {urls:?}");
    assert!(!urls.iter().any(|u| u.ends_with("/missing")), "links of the outside page are not followed: {urls:?}");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
