use scraper::{ElementRef, Html, Node, Selector};
use url::Url;

use crate::norm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Anchor,
    Image,
    Script,
    Stylesheet,
    Iframe,
    Canonical,
    Hreflang,
    Redirect,
}

impl LinkKind {
    pub fn slug(self) -> &'static str {
        match self {
            LinkKind::Anchor => "a",
            LinkKind::Image => "img",
            LinkKind::Script => "script",
            LinkKind::Stylesheet => "css",
            LinkKind::Iframe => "iframe",
            LinkKind::Canonical => "canonical",
            LinkKind::Hreflang => "hreflang",
            LinkKind::Redirect => "redirect",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    /// Normalized absolute URL (see `norm::key`).
    pub url: String,
    pub kind: LinkKind,
    /// Link text, image alt text, or the language code of an hreflang link.
    pub anchor: String,
    pub nofollow: bool,
    /// `LINK_*` bits describing the element.
    pub flags: u8,
}

/// An `<img>` without both width and height attributes.
pub const LINK_NO_SIZE: u8 = 1;

/// Everything extracted from one HTML document.
#[derive(Debug, Clone, Default)]
pub struct PageData {
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub meta_robots: Option<String>,
    pub canonical: Option<String>,
    pub h1: Vec<String>,
    pub h2: Vec<String>,
    pub lang: Option<String>,
    pub word_count: u32,
    pub dup_hash: u64,
    /// Page-level `nofollow` (meta robots): none of its anchors are followed.
    pub nofollow: bool,
    /// Structured data found: formats ("JSON-LD", "Microdata", "RDFa") and
    /// JSON-LD `@type`s, comma separated. `None` when there is none.
    pub structured_data: Option<String>,
    /// JSON-LD blocks that are not valid JSON.
    pub jsonld_errors: u32,
    /// Target of a `<meta http-equiv="refresh">` redirect.
    pub meta_refresh: Option<String>,
    /// The canonical tag's href was relative.
    pub canonical_relative: bool,
    /// Headings skip a level going down (an H2 followed by an H4).
    pub heading_skip: bool,
    pub has_viewport: bool,
    pub has_open_graph: bool,
    pub has_twitter_card: bool,
    /// Forms on an HTTPS page that submit to an HTTP address.
    pub insecure_forms: u32,
    /// SimHash of the main content (header, nav, footer and asides left
    /// out), for finding near-duplicate pages.
    pub content_simhash: u64,
    pub links: Vec<Link>,
}

fn sel(q: &str) -> Selector {
    Selector::parse(q).expect("static selector")
}

fn has_token(attr: Option<&str>, token: &str) -> bool {
    attr.is_some_and(|v| v.split_whitespace().any(|t| t.eq_ignore_ascii_case(token)))
}

fn text_of(e: ElementRef) -> String {
    norm::normalize_ws(&e.text().collect::<String>())
}

/// Visible text of the body: all text nodes outside script/style-like
/// elements. Iterative so deeply nested markup cannot overflow the stack.
fn visible_text(doc: &Html) -> String {
    body_text(doc, &[])
}

/// The page's own content: visible text without the site's chrome.
fn main_text(doc: &Html) -> String {
    body_text(doc, &["header", "nav", "footer", "aside"])
}

fn body_text(doc: &Html, also_skip: &[&str]) -> String {
    let root = doc
        .select(&sel("body"))
        .next()
        .unwrap_or_else(|| doc.root_element());
    let mut out = String::new();
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(t) => {
                out.push_str(t);
                out.push(' ');
            }
            Node::Element(e)
                if matches!(e.name(), "script" | "style" | "noscript" | "template" | "svg")
                    || also_skip.contains(&e.name()) => {}
            _ => stack.extend(node.children().rev()),
        }
    }
    norm::normalize_ws(&out)
}

pub fn parse_html(html: &str, page_url: &Url) -> PageData {
    let doc = Html::parse_document(html);

    let base = doc
        .select(&sel("base[href]"))
        .next()
        .and_then(|e| e.value().attr("href"))
        .and_then(|h| page_url.join(h.trim()).ok())
        .unwrap_or_else(|| page_url.clone());

    let title = doc
        .select(&sel("head title"))
        .next()
        .or_else(|| doc.select(&sel("title")).next())
        .map(text_of)
        .filter(|t| !t.is_empty());

    let meta = |name: &str| {
        doc.select(&sel("meta[name][content]"))
            .find(|e| {
                e.value()
                    .attr("name")
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(name))
            })
            .and_then(|e| e.value().attr("content"))
            .map(norm::normalize_ws)
    };
    let meta_description = meta("description");
    let meta_robots = meta("robots").map(|r| r.to_ascii_lowercase());
    let nofollow = meta_robots
        .as_deref()
        .is_some_and(|r| r.split(',').any(|d| matches!(d.trim(), "nofollow" | "none")));

    let lang = doc
        .root_element()
        .value()
        .attr("lang")
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());

    let h1 = doc.select(&sel("h1")).map(text_of).collect();
    let h2 = doc.select(&sel("h2")).map(text_of).collect();
    let heading_skip = {
        let levels: Vec<u8> = doc
            .select(&sel("h1, h2, h3, h4, h5, h6"))
            .filter_map(|e| e.value().name()[1..].parse().ok())
            .collect();
        levels.windows(2).any(|w| w[1] > w[0] + 1)
    };
    let meta_present = |attr: &str, prefix: &str| {
        doc.select(&sel("meta[content]")).any(|e| e.value().attr(attr).is_some_and(|v| v.trim().to_ascii_lowercase().starts_with(prefix)))
    };
    let has_viewport = meta_present("name", "viewport");
    let has_open_graph = meta_present("property", "og:");
    let has_twitter_card = meta_present("name", "twitter:card");
    let meta_refresh = doc
        .select(&sel("meta[http-equiv][content]"))
        .find(|e| e.value().attr("http-equiv").is_some_and(|v| v.trim().eq_ignore_ascii_case("refresh")))
        .and_then(|e| e.value().attr("content"))
        .and_then(refresh_target)
        .and_then(|t| norm::resolve_key(&base, &t));
    let https = page_url.scheme() == "https";
    let insecure_forms = doc
        .select(&sel("form"))
        .filter(|f| {
            let action = f.value().attr("action").unwrap_or("").trim();
            https && base.join(action).is_ok_and(|u| u.scheme() == "http")
        })
        .count() as u32;

    let mut links = Vec::new();
    let mut push = |href: Option<&str>, kind: LinkKind, anchor: String, nofollow: bool, flags: u8| {
        if let Some(url) = href.and_then(|h| norm::resolve_key(&base, h)) {
            links.push(Link { url, kind, anchor, nofollow, flags });
        }
    };
    if let Some(t) = &meta_refresh {
        push(Some(t.as_str()), LinkKind::Redirect, String::new(), false, 0);
    }

    let mut canonical = None;
    let mut canonical_relative = false;
    for e in doc.select(&sel("link[href]")) {
        let v = e.value();
        let rel = v.attr("rel");
        if has_token(rel, "canonical") {
            if let Some(u) = v.attr("href").and_then(|h| norm::resolve_key(&base, h)) {
                if canonical.is_none() {
                    let raw = v.attr("href").unwrap_or("").trim();
                    canonical_relative = !(raw.starts_with("http://") || raw.starts_with("https://"));
                }
                canonical.get_or_insert(u);
            }
            push(v.attr("href"), LinkKind::Canonical, String::new(), false, 0);
        } else if has_token(rel, "stylesheet") {
            push(v.attr("href"), LinkKind::Stylesheet, String::new(), false, 0);
        } else if has_token(rel, "alternate") && v.attr("hreflang").is_some() {
            let code = v.attr("hreflang").unwrap_or("").trim().to_string();
            push(v.attr("href"), LinkKind::Hreflang, code, false, 0);
        }
    }
    for e in doc.select(&sel("a[href], area[href]")) {
        let mut anchor = text_of(e);
        if anchor.is_empty() {
            anchor = e
                .select(&sel("img[alt]"))
                .next()
                .and_then(|i| i.value().attr("alt"))
                .map(norm::normalize_ws)
                .unwrap_or_default();
        }
        let nf = has_token(e.value().attr("rel"), "nofollow");
        push(e.value().attr("href"), LinkKind::Anchor, anchor, nf, 0);
    }
    for e in doc.select(&sel("img[src]")) {
        let v = e.value();
        let alt = v.attr("alt").map(norm::normalize_ws).unwrap_or_default();
        let sized = v.attr("width").is_some() && v.attr("height").is_some();
        push(v.attr("src"), LinkKind::Image, alt, false, if sized { 0 } else { LINK_NO_SIZE });
    }
    for e in doc.select(&sel("script[src]")) {
        push(e.value().attr("src"), LinkKind::Script, String::new(), false, 0);
    }
    for e in doc.select(&sel("iframe[src]")) {
        push(e.value().attr("src"), LinkKind::Iframe, String::new(), false, 0);
    }

    let (structured_data, jsonld_errors) = structured_data(&doc);
    let text = visible_text(&doc);
    PageData {
        title,
        meta_description,
        meta_robots,
        canonical,
        h1,
        h2,
        lang,
        word_count: norm::word_count(&text),
        dup_hash: norm::dup_hash(&text),
        nofollow,
        structured_data,
        jsonld_errors,
        meta_refresh,
        canonical_relative,
        heading_skip,
        has_viewport,
        has_open_graph,
        has_twitter_card,
        insecure_forms,
        content_simhash: simhash(&main_text(&doc)),
        links,
    }
}

/// The URL in a refresh value such as `5; url=/next`.
fn refresh_target(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let at = lower.find("url=")? + 4;
    let t = content[at..].trim().trim_matches(['"', '\'']).trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 64-bit SimHash over three-word shingles: similar texts get hashes that
/// differ in few bits.
pub fn simhash(text: &str) -> u64 {
    let words: Vec<String> = text.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return 0;
    }
    let mut votes = [0i32; 64];
    let shingles: Vec<String> = if words.len() < 3 { vec![words.join(" ")] } else { words.windows(3).map(|w| w.join(" ")).collect() };
    for sh in &shingles {
        let h = norm::fnv1a64(sh.as_bytes());
        for (bit, vote) in votes.iter_mut().enumerate() {
            *vote += if h >> bit & 1 == 1 { 1 } else { -1 };
        }
    }
    votes.iter().enumerate().fold(0u64, |acc, (bit, v)| if *v > 0 { acc | 1 << bit } else { acc })
}

/// Formats and JSON-LD types of the page's structured data, and how many
/// JSON-LD blocks failed to parse.
fn structured_data(doc: &Html) -> (Option<String>, u32) {
    let mut found: Vec<String> = vec![];
    let mut types: Vec<String> = vec![];
    let mut errors = 0;
    let mut has_jsonld = false;
    for e in doc.select(&sel("script[type]")) {
        if !e.value().attr("type").is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json")) {
            continue;
        }
        has_jsonld = true;
        match serde_json::from_str::<serde_json::Value>(&e.text().collect::<String>()) {
            Ok(v) => collect_types(&v, &mut types, 0),
            Err(_) => errors += 1,
        }
    }
    if has_jsonld {
        found.push("JSON-LD".into());
    }
    if doc.select(&sel("[itemscope]")).next().is_some() {
        found.push("Microdata".into());
    }
    if doc.select(&sel("[typeof], [vocab]")).next().is_some() {
        found.push("RDFa".into());
    }
    types.sort();
    types.dedup();
    found.extend(types);
    ((!found.is_empty()).then(|| found.join(", ")), errors)
}

fn collect_types(v: &serde_json::Value, out: &mut Vec<String>, depth: u32) {
    if depth > 4 {
        return;
    }
    match v {
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_types(i, out, depth + 1)),
        serde_json::Value::Object(map) => {
            match map.get("@type") {
                Some(serde_json::Value::String(t)) => out.push(t.clone()),
                Some(serde_json::Value::Array(ts)) => out.extend(ts.iter().filter_map(|t| t.as_str().map(String::from))),
                _ => {}
            }
            if let Some(g) = map.get("@graph") {
                collect_types(g, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// Approximate rendered width in pixels of `text` in Arial at `size_px`, the
/// face search results use. Widths are Arial's advance widths per 1000 em.
pub fn pixel_width(text: &str, size_px: f32) -> u32 {
    const ASCII: [u16; 95] = [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' '../
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
        278, 278, 584, 584, 584, 556, 1015, // :;<=>?@
        667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // A-M
        722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // N-Z
        278, 278, 278, 469, 556, 333, // [\]^_`
        556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // a-m
        556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // n-z
        334, 260, 334, 584, // {|}~
    ];
    let units: u32 = text
        .chars()
        .map(|c| match c as u32 {
            32..=126 => ASCII[(c as u32 - 32) as usize] as u32,
            _ if c.is_whitespace() => 278,
            _ => 556,
        })
        .sum();
    (units as f32 * size_px / 1000.0).round() as u32
}

/// True when a robots directive string (meta robots or X-Robots-Tag)
/// contains `noindex` / `none`.
pub fn is_noindex(directives: &str) -> bool {
    directives
        .to_ascii_lowercase()
        .split([',', ' '])
        .any(|d| matches!(d.trim(), "noindex" | "none"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r##"<!doctype html><html lang="en"><head>
        <title>  Hello
            World </title>
        <meta name="Description" content="A  test page">
        <meta name="robots" content="index, nofollow">
        <link rel="canonical" href="/canon">
        <link rel="stylesheet" href="/s.css">
        <link rel="alternate" hreflang="de" href="/de/">
        <script src="app.js"></script>
        </head><body>
        <h1>Main</h1><h2>Sub one</h2><h2>Sub two</h2>
        <p>Some visible words here.</p>
        <script>var hidden = "not counted";</script>
        <a href="/next">Next page</a>
        <a href="https://other.org/x" rel="external nofollow">Out</a>
        <a href="#top">Top</a><a href="mailto:a@b.c">Mail</a>
        <a href="/img-link"><img src="/i.png" alt="Logo"></a>
        </body></html>"##;

    #[test]
    fn extracts_fields_and_links() {
        let u = Url::parse("https://example.com/dir/page").unwrap();
        let p = parse_html(DOC, &u);
        assert_eq!(p.title.as_deref(), Some("Hello World"));
        assert_eq!(p.meta_description.as_deref(), Some("A test page"));
        assert!(p.nofollow);
        assert_eq!(p.canonical.as_deref(), Some("https://example.com/canon"));
        assert_eq!(p.h1, vec!["Main"]);
        assert_eq!(p.h2.len(), 2);
        assert_eq!(p.lang.as_deref(), Some("en"));
        // Main Sub one Sub two Some visible words here. Next page Out Top Mail
        assert_eq!(p.word_count, 14);

        let find = |url: &str| p.links.iter().find(|l| l.url == url).unwrap();
        assert_eq!(find("https://example.com/next").anchor, "Next page");
        assert!(find("https://other.org/x").nofollow);
        assert_eq!(find("https://example.com/dir/app.js").kind, LinkKind::Script);
        assert_eq!(find("https://example.com/s.css").kind, LinkKind::Stylesheet);
        assert_eq!(find("https://example.com/de/").kind, LinkKind::Hreflang);
        assert_eq!(find("https://example.com/img-link").anchor, "Logo");
        assert_eq!(find("https://example.com/i.png").kind, LinkKind::Image);
        assert!(!p.links.iter().any(|l| l.url.starts_with("mailto")));
    }

    #[test]
    fn base_href_is_respected() {
        let u = Url::parse("https://example.com/a/b").unwrap();
        let p = parse_html(
            r#"<html><head><base href="https://cdn.example.com/root/"></head>
               <body><a href="x">x</a></body></html>"#,
            &u,
        );
        assert_eq!(p.links[0].url, "https://cdn.example.com/root/x");
    }

    #[test]
    fn structured_data_found() {
        let u = Url::parse("https://example.com/").unwrap();
        let p = parse_html(
            r#"<html><head><script type="application/ld+json">{"@context":"https://schema.org","@graph":[{"@type":"Organization"},{"@type":["WebSite","Thing"]}]}</script></head>
               <body><div itemscope itemtype="https://schema.org/Product"></div></body></html>"#,
            &u,
        );
        assert_eq!(p.structured_data.as_deref(), Some("JSON-LD, Microdata, Organization, Thing, WebSite"));
        assert_eq!(parse_html("<p>x</p>", &u).structured_data, None);
    }

    #[test]
    fn pixel_widths() {
        assert_eq!(pixel_width("", 20.0), 0);
        // "Hello" = 722+556+222+222+556 = 2278 units -> 45.56 px at 20px
        assert_eq!(pixel_width("Hello", 20.0), 46);
        assert!(pixel_width("WWW", 20.0) > pixel_width("iii", 20.0));
    }

    #[test]
    fn page_signals() {
        let u = Url::parse("https://example.com/").unwrap();
        let p = parse_html(
            r#"<html><head><meta http-equiv="Refresh" content="0; URL='/next'"><meta name="viewport" content="width=device-width">
               <meta property="og:title" content="T"><link rel="canonical" href="/c">
               <link rel="alternate" hreflang="de-DE" href="/de/"><script type="application/ld+json">{oops</script></head>
               <body><header><p>menu menu menu</p></header><h1>A</h1><h3>B</h3><img src="/a.png"><img src="/b.png" width="1" height="1">
               <form action="http://example.com/post"></form><p>the real content of the page</p></body></html>"#,
            &u,
        );
        assert_eq!(p.meta_refresh.as_deref(), Some("https://example.com/next"));
        assert!(p.canonical_relative && p.heading_skip && p.has_viewport && p.has_open_graph && !p.has_twitter_card);
        assert_eq!(p.jsonld_errors, 1);
        assert_eq!(p.insecure_forms, 1);
        let img = |s: &str| p.links.iter().find(|l| l.url.ends_with(s)).unwrap().flags;
        assert_eq!((img("/a.png"), img("/b.png")), (LINK_NO_SIZE, 0));
        assert_eq!(p.links.iter().find(|l| l.kind == LinkKind::Hreflang).unwrap().anchor, "de-DE");
        assert!(p.links.iter().any(|l| l.kind == LinkKind::Redirect));
    }

    #[test]
    fn simhash_tracks_similarity() {
        let a = "the quick brown fox jumps over the lazy dog near the river bank today";
        let b = "the quick brown fox jumps over the lazy dog near the river bank tonight";
        let c = "completely different words about something else entirely unrelated to foxes";
        let d = |x: u64, y: u64| (x ^ y).count_ones();
        assert!(d(simhash(a), simhash(b)) < d(simhash(a), simhash(c)));
        assert_eq!(simhash(""), 0);
    }

    #[test]
    fn noindex_detection() {
        assert!(is_noindex("NOINDEX, follow"));
        assert!(is_noindex("none"));
        assert!(!is_noindex("index, follow"));
    }
}
