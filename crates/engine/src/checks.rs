//! The check catalogue: report views, their columns and filters, and which
//! filters are problems. Every filter is a SQL condition over the `urls`
//! table, applied together with its view's scope. Filters carry a stable
//! slug, so a problem keeps its identity (slug + URL) from crawl to crawl.

/// A column a results table can show.
pub struct Col {
    pub id: &'static str,
    pub title: &'static str,
    /// SQL expression over `urls`.
    pub expr: &'static str,
    pub numeric: bool,
    pub width: f32,
}

macro_rules! col {
    ($id:literal, $title:literal, $expr:expr, num, $w:literal) => {
        Col { id: $id, title: $title, expr: $expr, numeric: true, width: $w }
    };
    ($id:literal, $title:literal, $expr:expr, $w:literal) => {
        Col { id: $id, title: $title, expr: $expr, numeric: false, width: $w }
    };
}

/// Reason phrases for common status codes; others show the code only.
const STATUS_TEXT: &str = "CASE status \
    WHEN 200 THEN 'OK' WHEN 201 THEN 'Created' WHEN 204 THEN 'No Content' \
    WHEN 301 THEN 'Moved Permanently' WHEN 302 THEN 'Found' WHEN 303 THEN 'See Other' \
    WHEN 304 THEN 'Not Modified' WHEN 307 THEN 'Temporary Redirect' WHEN 308 THEN 'Permanent Redirect' \
    WHEN 400 THEN 'Bad Request' WHEN 401 THEN 'Unauthorized' WHEN 403 THEN 'Forbidden' \
    WHEN 404 THEN 'Not Found' WHEN 405 THEN 'Method Not Allowed' WHEN 408 THEN 'Request Timeout' \
    WHEN 410 THEN 'Gone' WHEN 429 THEN 'Too Many Requests' WHEN 500 THEN 'Internal Server Error' \
    WHEN 502 THEN 'Bad Gateway' WHEN 503 THEN 'Service Unavailable' WHEN 504 THEN 'Gateway Timeout' \
    ELSE CASE WHEN status IS NULL THEN indexability ELSE '' END END";

/// Inbound link count, from the table the store keeps current.
const INLINKS: &str = "COALESCE((SELECT i.total FROM inlinks i WHERE i.url = urls.url), 0)";

pub const COLS: &[Col] = &[
    col!("address", "Address", "url", 380.0),
    col!("content_type", "Content Type", "content_type", 170.0),
    col!("status_code", "Status Code", "status", num, 90.0),
    col!("status", "Status", STATUS_TEXT, 140.0),
    col!("indexability", "Indexability", "CASE indexable WHEN 1 THEN 'Indexable' ELSE 'Non-Indexable' END", 110.0),
    col!("indexability_status", "Indexability Status", "CASE WHEN indexable = 1 THEN '' ELSE indexability END", 150.0),
    col!("title", "Title 1", "title", 280.0),
    col!("title_len", "Title 1 Length", "title_len", num, 105.0),
    col!("title_px", "Title 1 Pixel Width", "title_px", num, 130.0),
    col!("meta", "Meta Description 1", "meta_description", 300.0),
    col!("meta_len", "Meta Description 1 Length", "meta_description_len", num, 170.0),
    col!("meta_px", "Meta Description 1 Pixel Width", "meta_description_px", num, 200.0),
    col!("h1", "H1-1", "h1", 260.0),
    col!("h1_len", "H1-1 Length", "length(h1)", num, 90.0),
    col!("h1_count", "H1 Count", "h1_count", num, 75.0),
    col!("h2", "H2-1", "h2", 260.0),
    col!("h2_len", "H2-1 Length", "length(h2)", num, 90.0),
    col!("h2_count", "H2 Count", "h2_count", num, 75.0),
    col!("meta_robots", "Meta Robots 1", "meta_robots", 160.0),
    col!("x_robots", "X-Robots-Tag 1", "x_robots_tag", 160.0),
    col!("meta_refresh", "Meta Refresh 1", "meta_refresh", 260.0),
    col!("canonical", "Canonical Link Element 1", "canonical", 320.0),
    col!("word_count", "Word Count", "word_count", num, 85.0),
    col!("near_dups", "No. Near Duplicates", "near_duplicates", num, 135.0),
    col!("size", "Size (Bytes)", "size", num, 95.0),
    col!("response_ms", "Response Time (ms)", "response_ms", num, 130.0),
    // Pages found only through the sitemap have no link depth.
    col!("depth", "Crawl Depth", "CASE WHEN depth >= 0 THEN depth END", num, 90.0),
    col!("inlinks", "Inlinks", INLINKS, num, 70.0),
    col!("outlinks", "Outlinks", "out_internal", num, 75.0),
    col!("ext_outlinks", "External Outlinks", "out_external", num, 120.0),
    col!("link_score", "Link Score", "link_score", num, 85.0),
    col!("found_via", "Found Via", "CASE found_via WHEN 'start' THEN 'Start URL' WHEN 'link' THEN 'Link' WHEN 'redirect' THEN 'Redirect' WHEN 'meta refresh' THEN 'Meta Refresh' WHEN 'canonical' THEN 'Canonical' WHEN 'hreflang' THEN 'Hreflang' WHEN 'resource' THEN 'Resource' WHEN 'sitemap' THEN 'XML Sitemap' ELSE found_via END", 100.0),
    col!("redirect_to", "Redirect URL", "redirect_to", 320.0),
    col!("url_len", "Length", "length(url)", num, 70.0),
    col!("in_sitemap", "In Sitemap", "CASE in_sitemap WHEN 1 THEN 'Yes' ELSE 'No' END", 85.0),
    col!("sitemap_lastmod", "Sitemap Lastmod", "sitemap_lastmod", 160.0),
    col!("rendered", "Rendered", "CASE rendered WHEN 1 THEN 'Yes' ELSE 'No' END", 80.0),
    col!("js_links", "JavaScript-Only Links", "js_only_links", num, 145.0),
    col!("js_words", "JavaScript-Only Words", "js_only_words", num, 150.0),
    col!("render_error", "Rendering Error", "render_error", 260.0),
    col!("structured_data", "Structured Data Formats", "structured_data", 260.0),
    col!("hreflang", "Hreflang Links", "hreflang_links", num, 110.0),
    col!("missing_alt", "Pages Missing Alt", "(SELECT COUNT(DISTINCT l.from_url) FROM links l WHERE l.to_url = urls.url AND l.kind = 'img' AND l.anchor = '')", num, 125.0),
    col!("broken_links", "Broken Outlinks", "(SELECT COUNT(*) FROM links l WHERE l.from_url = urls.url AND l.to_url IN (SELECT t.url FROM urls t WHERE t.status >= 400 OR (t.status IS NULL AND t.robots_blocked = 0)))", num, 115.0),
    col!("error", "Error", "error", 260.0),
    col!("crawled", "Crawl Timestamp", "datetime(crawled_at, 'unixepoch', 'localtime')", 150.0),
];

pub fn col(id: &str) -> &'static Col {
    COLS.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("unknown column {id}"))
}

/// How bad a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Broken: fix it.
    Error,
    /// Likely hurting the site; check it.
    Warning,
    /// Worth knowing; may not need a fix.
    Notice,
}

impl Severity {
    pub const ALL: [Severity; 3] = [Self::Error, Self::Warning, Self::Notice];

    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Notice => "Notice",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Self::Error => "Errors",
            Self::Warning => "Warnings",
            Self::Notice => "Notices",
        }
    }

    /// Parse "error", "warning" or "notice" (for the command line).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("tip") {
            return Some(Self::Notice);
        }
        Self::ALL.into_iter().find(|v| v.label().eq_ignore_ascii_case(s))
    }
}

/// How much fixing it matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Impact {
    High,
    Medium,
    Low,
}

impl Impact {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

pub struct Problem {
    pub severity: Severity,
    pub impact: Impact,
    pub about: &'static str,
    pub fix: &'static str,
}

pub struct Filter {
    /// Stable identifier, e.g. "page_titles.missing".
    pub slug: &'static str,
    pub label: &'static str,
    pub sql: &'static str,
    pub problem: Option<Problem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Site,
    OnPage,
    Indexing,
    Linking,
}

impl Section {
    pub const ALL: [Section; 4] = [Self::Site, Self::OnPage, Self::Indexing, Self::Linking];

    pub fn label(self) -> &'static str {
        match self {
            Self::Site => "Site",
            Self::OnPage => "On-page",
            Self::Indexing => "Indexing",
            Self::Linking => "Linking",
        }
    }
}

pub struct View {
    pub id: &'static str,
    pub title: &'static str,
    pub section: Section,
    /// Rows this view is about; filters narrow it further.
    pub scope: &'static str,
    pub columns: &'static [&'static str],
    pub filters: &'static [Filter],
}

const fn f(slug: &'static str, label: &'static str, sql: &'static str) -> Filter {
    Filter { slug, label, sql, problem: None }
}

const fn p(
    slug: &'static str,
    label: &'static str,
    sql: &'static str,
    severity: Severity,
    impact: Impact,
    about: &'static str,
    fix: &'static str,
) -> Filter {
    Filter { slug, label, sql, problem: Some(Problem { severity, impact, about, fix }) }
}

use Impact::{High, Low, Medium};
use Severity::{Error, Notice, Warning};

/// Internal HTML pages that answered 200: the population on-page checks use.
const PAGES: &str = "internal = 1 AND resource_type = 'html' AND status = 200";
/// Meta robots and X-Robots-Tag together, lowercased.
macro_rules! dir {
    () => {
        "(',' || lower(COALESCE(meta_robots, '') || ',' || COALESCE(x_robots_tag, '')) || ',')"
    };
}
macro_rules! has_dir {
    ($d:literal) => {
        concat!(dir!(), " REGEXP '[ ,]", $d, "[ ,:]'")
    };
}
macro_rules! header_missing {
    ($h:literal) => {
        concat!(
            "url LIKE 'https://%' AND resource_type = 'html' AND status = 200 AND headers IS NOT NULL AND lower(headers) NOT LIKE '%",
            $h,
            ":%'"
        )
    };
}
macro_rules! dup_of {
    ($col:literal) => {
        concat!(
            $col, " IN (SELECT ", $col, " FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200 AND ",
            $col, " IS NOT NULL AND ", $col, " <> '' GROUP BY ", $col, " HAVING COUNT(*) > 1)"
        )
    };
}
/// `scheme://host` of a URL column, for same-site comparisons.
macro_rules! origin_of {
    ($c:literal) => {
        concat!("substr(", $c, ", 1, instr(substr(", $c, ", 9) || '/', '/') + 7)")
    };
}

/// URLs that answered with an error or not at all.
macro_rules! broken {
    () => {
        "SELECT t.url FROM urls t WHERE t.status >= 400 OR (t.status IS NULL AND t.robots_blocked = 0)"
    };
}

pub const VIEWS: &[View] = &[
    // ---- Site ---------------------------------------------------------------
    View {
        id: "internal",
        title: "Internal",
        section: Section::Site,
        scope: "internal = 1",
        columns: &["address", "content_type", "status_code", "status", "indexability", "indexability_status", "title", "title_len", "meta", "meta_len", "h1", "word_count", "size", "response_ms", "depth", "inlinks", "outlinks", "link_score", "found_via", "in_sitemap", "crawled"],
        filters: &[
            f("internal.all", "All", "1"),
            f("internal.html", "HTML", "resource_type = 'html'"),
            f("internal.javascript", "JavaScript", "resource_type = 'javascript'"),
            f("internal.css", "CSS", "resource_type = 'css'"),
            f("internal.images", "Images", "resource_type = 'image'"),
            f("internal.pdf", "PDF", "resource_type = 'pdf'"),
            f("internal.other", "Other", "resource_type IN ('other', 'redirect', 'noresponse')"),
        ],
    },
    View {
        id: "external",
        title: "External",
        section: Section::Site,
        scope: "internal = 0",
        columns: &["address", "content_type", "status_code", "status", "inlinks", "response_ms", "crawled"],
        filters: &[
            f("external.all", "All", "1"),
            f("external.html", "HTML", "resource_type = 'html'"),
            f("external.resources", "JavaScript, CSS and Images", "resource_type IN ('javascript', 'css', 'image')"),
            p("external.broken", "Broken (4xx, 5xx, No Response)", "status >= 400 OR (status IS NULL AND robots_blocked = 0)", Warning, Medium,
              "Links to other sites that no longer work. Visitors who follow them hit an error page.",
              "Update the links to a working address or remove them. Inlinks shows which pages carry them."),
        ],
    },
    View {
        id: "response_codes",
        title: "Response Codes",
        section: Section::Site,
        scope: "1",
        columns: &["address", "content_type", "status_code", "status", "indexability", "indexability_status", "inlinks", "response_ms", "redirect_to", "meta_refresh", "error"],
        filters: &[
            f("response_codes.all", "All", "1"),
            p("response_codes.blocked", "Blocked by Robots.txt", "robots_blocked = 1", Warning, High,
              "URLs robots.txt does not allow the crawler to fetch, so search engines cannot read them either.",
              "Confirm each should be blocked. If not, change the robots.txt rule that matches it."),
            p("response_codes.no_response", "No Response", "internal = 1 AND status IS NULL AND robots_blocked = 0", Error, High,
              "Internal URLs that never answered: timeouts, refused connections or DNS failures.",
              "Retry with a slower crawl or a longer timeout (Configuration > Speed and Advanced). Malformed links need fixing at the source."),
            f("response_codes.success", "Success (2xx)", "status BETWEEN 200 AND 299"),
            p("response_codes.redirection", "Redirection (3xx)", "internal = 1 AND status BETWEEN 300 AND 399", Warning, Low,
              "Internal URLs that send visitors somewhere else. Every link to one costs an extra round trip.",
              "Point links straight at the final URL. Inlinks shows where each redirect is linked from."),
            p("response_codes.meta_refresh", "Redirection (Meta Refresh)", "internal = 1 AND meta_refresh IS NOT NULL", Warning, Low,
              "Pages that redirect with a meta refresh tag instead of an HTTP redirect.",
              "Use a 301 redirect on the server, and link to the destination directly."),
            p("response_codes.redirect_chain", "Redirect Chain", "internal = 1 AND status BETWEEN 300 AND 399 AND redirect_to IN (SELECT r.url FROM urls r WHERE r.status BETWEEN 300 AND 399)", Warning, Medium,
              "Redirects that lead to another redirect before reaching a page.",
              "Point the first URL straight at the final destination. Bulk Export > Reports > Redirect Chains lists every hop."),
            p("response_codes.client_error", "Client Error (4xx)", "internal = 1 AND status BETWEEN 400 AND 499", Error, High,
              "Internal URLs that answer 404 Not Found, 410 Gone or another client error.",
              "Fix or remove the links to them, or redirect each to its closest replacement."),
            p("response_codes.server_error", "Server Error (5xx)", "internal = 1 AND status >= 500", Error, High,
              "Internal URLs where the server failed to answer. Search engines drop pages that keep failing.",
              "Check the server's error log for these URLs. Errors that come and go often mean an overloaded server."),
            f("response_codes.errors", "All Errors (4xx, 5xx, No Response)", "status >= 400 OR (status IS NULL AND robots_blocked = 0)"),
        ],
    },
    View {
        id: "url",
        title: "URL",
        section: Section::Site,
        scope: "internal = 1 AND resource_type = 'html'",
        columns: &["address", "status_code", "indexability", "url_len"],
        filters: &[
            f("url.all", "All", "1"),
            p("url.non_ascii", "Non ASCII Characters", "url_path(url) REGEXP '[^\\x00-\\x7F]'", Warning, Low,
              "Addresses with characters outside plain ASCII. They are percent-encoded when shared and break in some tools.",
              "Use plain ASCII words in new URLs and keep old ones working with redirects."),
            p("url.underscores", "Underscores", "url_path(url) LIKE '%\\_%' ESCAPE '\\'", Notice, Low,
              "Addresses that separate words with underscores. Hyphens are the common choice and read better.",
              "Use hyphens in new URLs; renaming old ones is rarely worth the redirects."),
            p("url.uppercase", "Uppercase", "url_path(url) REGEXP '\\p{Lu}'", Notice, Low,
              "Addresses with capitals. URLs are case-sensitive, so one page can end up with several addresses.",
              "Use lowercase and redirect mixed-case variants."),
            p("url.parameters", "Parameters", "url LIKE '%?%'", Warning, Low,
              "Addresses with query strings, which can multiply into many URLs for the same content.",
              "Link to clean URLs and give parameter variants a canonical tag."),
            p("url.over_115", "Over 115 Characters", "length(url) > 115", Notice, Low,
              "Long addresses are hard to read, share and remember, and are cut short in results.",
              "Keep new URLs short and descriptive."),
            p("url.multiple_slashes", "Multiple Slashes", "url_path(url) LIKE '%//%'", Error, Low,
              "Paths with doubled slashes, usually a template bug, which often duplicate another URL.",
              "Fix the links producing them and redirect to the single-slash address."),
            p("url.space", "Contains Space", "url_path(url) LIKE '% %'", Warning, Low,
              "Addresses with spaces, which become %20 and break when copied.",
              "Replace spaces with hyphens and redirect the old addresses."),
            p("url.internal_search", "Internal Search", "url REGEXP '(?i)[?&](q|s|query|search|keyword)=|/search[/?]'", Warning, Low,
              "Pages of the site's own search. They are thin, near-endless and seldom worth crawling.",
              "Keep them out of the index with noindex and avoid linking to them from crawlable pages."),
            p("url.tracking", "Tracking Parameters", "url REGEXP '(?i)[?&](utm_[a-z]+|gclid|fbclid)='", Warning, Low,
              "Internal links carrying campaign parameters. They duplicate URLs and overwrite the visit's real source.",
              "Strip tracking parameters from internal links; keep them for links from other sites."),
        ],
    },
    View {
        id: "security",
        title: "Security",
        section: Section::Site,
        scope: "internal = 1",
        columns: &["address", "status_code", "indexability", "content_type"],
        filters: &[
            f("security.all", "All", "1"),
            p("security.http", "HTTP URLs", "url LIKE 'http://%'", Error, High,
              "Internal URLs served without encryption. Browsers mark them not secure and traffic can be read in transit.",
              "Serve everything over HTTPS, redirect HTTP to HTTPS, and update internal links."),
            f("security.https", "HTTPS URLs", "url LIKE 'https://%'"),
            p("security.mixed_content", "Mixed Content", "insecure_resources > 0", Error, High,
              "HTTPS pages loading images, scripts, stylesheets or frames over HTTP. Browsers block or flag them.",
              "Load every resource over HTTPS. The Resources tab lists the HTTP ones."),
            p("security.insecure_form", "Form on HTTPS Posts to HTTP", "insecure_forms > 0", Error, High,
              "HTTPS pages with a form that sends what people type over plain HTTP.",
              "Point every form action at an HTTPS address."),
            p("security.hsts_missing", "Missing HSTS Header", header_missing!("strict-transport-security"), Warning, Low,
              "HTTPS pages without Strict-Transport-Security, so browsers may still try plain HTTP first.",
              "Send Strict-Transport-Security with a long max-age once the whole site works over HTTPS."),
            p("security.hsts_short", "HSTS Max-Age Under 6 Months", "url LIKE 'https://%' AND resource_type = 'html' AND hsts_max_age(headers) < 15552000", Warning, Low,
              "HSTS headers with a short max-age, which protects visitors for only a short while.",
              "Use a max-age of at least 15552000 seconds (180 days); a year is common."),
            p("security.csp_missing", "Missing Content-Security-Policy", header_missing!("content-security-policy"), Notice, Low,
              "Pages without a Content-Security-Policy to limit where scripts may load from.",
              "Add a policy that allows only the origins the site needs; start in report-only mode."),
            p("security.csp_unsafe", "CSP Allows unsafe-inline or unsafe-eval", "lower(headers) REGEXP 'content-security-policy:[^\\n]*unsafe-(inline|eval)'", Warning, Low,
              "Content-Security-Policies that still allow inline scripts or eval, which removes most of their protection.",
              "Replace 'unsafe-inline' with nonces or hashes and remove the code that needs 'unsafe-eval'."),
            p("security.nosniff", "Missing X-Content-Type-Options", header_missing!("x-content-type-options"), Warning, Low,
              "Pages without X-Content-Type-Options: nosniff, so browsers may guess content types.",
              "Send X-Content-Type-Options: nosniff on every response."),
            p("security.framing", "Missing X-Frame-Options", "url LIKE 'https://%' AND resource_type = 'html' AND status = 200 AND headers IS NOT NULL AND lower(headers) NOT LIKE '%x-frame-options:%' AND lower(headers) NOT LIKE '%frame-ancestors%'", Warning, Low,
              "Pages any site may embed in a frame, which enables clickjacking.",
              "Send X-Frame-Options: SAMEORIGIN or a frame-ancestors rule in the Content-Security-Policy."),
            p("security.referrer", "Missing Referrer-Policy", header_missing!("referrer-policy"), Notice, Low,
              "Pages without a Referrer-Policy, so full URLs may leak to other sites.",
              "Send Referrer-Policy: strict-origin-when-cross-origin or stricter."),
            p("security.permissions", "Missing Permissions-Policy", header_missing!("permissions-policy"), Notice, Low,
              "Pages that do not restrict browser features such as camera, microphone or location for embedded content.",
              "Send a Permissions-Policy that turns off the features the site does not use."),
            p("security.cookie_secure", "Cookie Without Secure", "url LIKE 'https://%' AND cookies_missing(headers, 'secure') > 0", Warning, Low,
              "Pages that set cookies without the Secure flag, so the cookie may travel over plain HTTP.",
              "Add Secure to every cookie set over HTTPS."),
            p("security.cookie_httponly", "Cookie Without HttpOnly", "cookies_missing(headers, 'httponly') > 0", Warning, Low,
              "Pages that set cookies scripts can read, which helps attackers who get a script onto the page.",
              "Add HttpOnly to every cookie no script needs."),
            p("security.cookie_samesite", "Cookie Without SameSite", "cookies_missing(headers, 'samesite') > 0", Notice, Low,
              "Pages that set cookies without a SameSite policy.",
              "Add SameSite=Lax (or Strict) to cookies."),
            p("security.server_version", "Server Version Disclosed", "lower(headers) REGEXP '(^|\\n)(server: [^\\n]*[0-9]|x-powered-by:)'", Notice, Low,
              "Responses that announce the server software and version, which helps attackers pick exploits.",
              "Drop the version from the Server header and remove X-Powered-By."),
        ],
    },
    View {
        id: "performance",
        title: "Performance",
        section: Section::Site,
        scope: "internal = 1 AND resource_type = 'html' AND status = 200",
        columns: &["address", "response_ms", "size", "word_count", "indexability"],
        filters: &[
            f("performance.all", "All", "1"),
            p("performance.slow", "Response Time Over 1 Second", "response_ms > 1000 AND response_ms <= 3000", Warning, Medium,
              "Pages the server took more than a second to send. Slow pages lose visitors and crawl budget.",
              "Cache the pages, speed up database queries, or use a CDN."),
            p("performance.very_slow", "Response Time Over 3 Seconds", "response_ms > 3000", Error, High,
              "Pages the server took more than three seconds to send.",
              "Find what makes these pages slow on the server; caching usually helps most."),
            p("performance.large_html", "HTML Over 1 MB", "size > 1048576", Warning, Medium,
              "Pages whose HTML alone is over 1 MB, which is slow to download and parse.",
              "Remove inlined data, long lists and embedded assets from the HTML."),
        ],
    },
    // ---- On-page --------------------------------------------------------------
    View {
        id: "page_titles",
        title: "Page Titles",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "title", "title_len", "title_px", "indexability"],
        filters: &[
            f("page_titles.all", "All", "1"),
            p("page_titles.missing", "Missing", "title IS NULL OR title = ''", Error, Medium,
              "Pages without a title. The title is the headline of a search result.",
              "Give every page a unique title that leads with its main topic."),
            p("page_titles.duplicate", "Duplicate", dup_of!("title"), Warning, Medium,
              "Pages whose title another page also uses, so results look alike.",
              "Write a distinct title for each page, or merge pages that really are the same."),
            p("page_titles.over_60", "Over 60 Characters", "title_len > 60", Notice, Medium,
              "Titles beyond roughly 60 characters are usually cut off in results.",
              "Put the important words first and trim the rest."),
            p("page_titles.below_30", "Below 30 Characters", "title_len < 30", Notice, Medium,
              "Very short titles leave out words that would describe the page.",
              "Expand the title so it says what the page offers."),
            p("page_titles.over_580px", "Over 580 Pixels", "title_px > 580", Notice, Medium,
              "Titles wider than about 580 pixels are truncated on desktop results.",
              "Shorten the title; the SERP Preview tab shows how it will look."),
            p("page_titles.below_200px", "Below 200 Pixels", "title_px < 200", Notice, Low,
              "Titles using a small part of the space a result offers.",
              "Use the room to describe the page more fully."),
            p("page_titles.same_as_h1", "Same as H1", "lower(title) = lower(h1)", Notice, Low,
              "Pages whose title repeats the main heading word for word.",
              "Consider wording that complements the heading."),
        ],
    },
    View {
        id: "meta_description",
        title: "Meta Description",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "meta", "meta_len", "meta_px", "indexability"],
        filters: &[
            f("meta_description.all", "All", "1"),
            p("meta_description.missing", "Missing", "meta_description IS NULL OR meta_description = ''", Notice, Low,
              "Pages without a meta description, so search engines pick a snippet themselves.",
              "Write a short summary that would make someone click."),
            p("meta_description.duplicate", "Duplicate", dup_of!("meta_description"), Warning, Low,
              "Pages whose description another page also uses.",
              "Give each page its own description."),
            p("meta_description.over_155", "Over 155 Characters", "meta_description_len > 155", Notice, Low,
              "Descriptions beyond about 155 characters are usually cut off.",
              "Keep the key message in the first 150 characters."),
            p("meta_description.below_70", "Below 70 Characters", "meta_description_len < 70", Notice, Low,
              "Very short descriptions miss the chance to explain the page.",
              "Expand the description into a proper summary."),
            p("meta_description.over_990px", "Over 990 Pixels", "meta_description_px > 990", Notice, Low,
              "Descriptions wider than about 990 pixels are truncated on desktop.",
              "Shorten it; check the SERP Preview tab."),
            p("meta_description.below_400px", "Below 400 Pixels", "meta_description_px < 400", Notice, Low,
              "Descriptions that fill little of the snippet.",
              "Use the space to describe the page."),
        ],
    },
    View {
        id: "h1",
        title: "H1",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "h1", "h1_len", "h1_count", "indexability"],
        filters: &[
            f("h1.all", "All", "1"),
            p("h1.missing", "Missing", "h1_count = 0", Error, Medium,
              "Pages without an H1. The main heading tells readers and crawlers what the page is about.",
              "Add one descriptive H1 to each page."),
            p("h1.empty", "Empty", "h1_count > 0 AND COALESCE(h1, '') = ''", Warning, Medium,
              "Pages whose first H1 has no text.",
              "Give the heading text, or remove the empty element."),
            p("h1.duplicate", "Duplicate", dup_of!("h1"), Notice, Low,
              "Pages whose H1 another page also uses.",
              "Give each page a heading about its own content."),
            p("h1.over_70", "Over 70 Characters", "length(h1) > 70", Notice, Low,
              "Long main headings are hard to scan.",
              "Keep the H1 short."),
            p("h1.multiple", "Multiple", "h1_count > 1", Warning, Medium,
              "Pages with several H1s, which blurs the main topic.",
              "Keep one H1 and use H2 to H6 for sections."),
        ],
    },
    View {
        id: "h2",
        title: "H2",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "h2", "h2_len", "h2_count", "indexability"],
        filters: &[
            f("h2.all", "All", "1"),
            p("h2.missing", "Missing", "h2_count = 0", Warning, Low,
              "Pages without subheadings, which makes long content hard to scan.",
              "Break longer content into sections with H2s."),
            p("h2.duplicate", "Duplicate", dup_of!("h2"), Notice, Low,
              "Pages whose first H2 matches another page's.",
              "Check the headings describe each page's own content."),
            p("h2.over_70", "Over 70 Characters", "length(h2) > 70", Notice, Low,
              "Long subheadings are hard to scan.",
              "Keep subheadings short."),
            p("h2.non_sequential", "Non-Sequential Headings", "heading_skip = 1", Notice, Low,
              "Pages whose headings skip a level, such as an H2 followed by an H4, which confuses screen readers.",
              "Step heading levels one at a time."),
        ],
    },
    View {
        id: "content",
        title: "Content",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "word_count", "near_dups", "size", "indexability"],
        filters: &[
            f("content.all", "All", "1"),
            p("content.exact_duplicates", "Exact Duplicates", "dup_hash IN (SELECT dup_hash FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200 GROUP BY dup_hash HAVING COUNT(*) > 1)", Error, Medium,
              "Pages whose visible text is exactly the same as another page's. Search engines index one and ignore the rest.",
              "Merge them with redirects or canonical tags, or give each its own content. The Duplicates tab lists the matches."),
            p("content.near_duplicates", "Near Duplicates", "near_duplicates > 0", Warning, Medium,
              "Pages whose main content (navigation, header and footer left out) is about 95% the same as another page's.",
              "Merge pages that say the same thing, or make each one distinct."),
            p("content.low_content", "Low Content Pages", "word_count < 200", Notice, Medium,
              "Pages with fewer than 200 words of visible text.",
              "Check they give people enough; expand or merge thin pages."),
            p("content.soft_404", "Soft 404 Pages", "(COALESCE(title, '') || ' ' || COALESCE(h1, '')) REGEXP '(?i)not found|\\b404\\b|doesn.t exist|no longer (exists|available)'", Warning, High,
              "Pages that answer 200 OK but read like an error page. Search engines treat them as errors anyway.",
              "Return a real 404 or 410 for missing content, or redirect to something relevant."),
            p("content.no_viewport", "Missing Viewport", "has_viewport = 0", Warning, Medium,
              "Pages without a viewport meta tag, which phones then show zoomed out like a desktop page.",
              "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">."),
            p("content.no_lang", "Missing Language", "lang IS NULL", Warning, Low,
              "Pages whose <html> has no lang attribute, so browsers and screen readers must guess the language.",
              "Add lang to the html element, for example lang=\"en\"."),
        ],
    },
    View {
        id: "images",
        title: "Images",
        section: Section::OnPage,
        // Anything used as an image counts, even when it failed and the
        // server answered with an HTML error page.
        scope: "resource_type = 'image' OR EXISTS (SELECT 1 FROM inlinks i WHERE i.url = urls.url AND i.images > 0)",
        columns: &["address", "content_type", "size", "status_code", "status", "inlinks", "missing_alt"],
        filters: &[
            f("images.all", "All", "1"),
            p("images.over_100kb", "Over 100 KB", "size > 102400", Notice, Medium,
              "Images over 100 KB, which slow down the pages showing them.",
              "Compress them, serve WebP or AVIF, and size them for how they are displayed."),
            p("images.missing_alt", "Missing Alt Text", "EXISTS (SELECT 1 FROM links l WHERE l.to_url = urls.url AND l.kind = 'img' AND l.anchor = '')", Error, Low,
              "Images shown on at least one page with no alt text, so screen readers have nothing to say.",
              "Describe meaningful images in alt; mark decorative ones with alt=\"\" deliberately."),
            p("images.alt_over_100", "Alt Text Over 100 Characters", "EXISTS (SELECT 1 FROM links l WHERE l.to_url = urls.url AND l.kind = 'img' AND length(l.anchor) > 100)", Notice, Low,
              "Images with very long alt text, which is tiring to listen to.",
              "Keep alt text to a short description."),
            p("images.missing_size", "Missing Size Attributes", "EXISTS (SELECT 1 FROM links l WHERE l.to_url = urls.url AND l.kind = 'img' AND (l.flags & 1) = 1)", Notice, Low,
              "Images placed without width and height, so the page jumps around while they load.",
              "Set width and height on each img so the browser can reserve the space."),
            p("images.broken", "Broken", "status >= 400 OR (status IS NULL AND robots_blocked = 0)", Error, Medium,
              "Images that fail to load, leaving gaps on the pages that use them.",
              "Fix the image address or upload the missing file."),
        ],
    },
    View {
        id: "structured_data",
        title: "Structured Data",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "structured_data", "indexability"],
        filters: &[
            f("structured_data.all", "All", "1"),
            f("structured_data.contains", "Contains Structured Data", "structured_data IS NOT NULL"),
            p("structured_data.missing", "Missing", "structured_data IS NULL", Notice, Low,
              "Pages without structured data, which can make them eligible for richer results.",
              "Add JSON-LD that matches the content, such as Article, Product or Organization."),
            p("structured_data.parse_errors", "JSON-LD Parse Errors", "jsonld_errors > 0", Error, Medium,
              "Pages with a JSON-LD block that is not valid JSON. Search engines ignore the whole block.",
              "Fix the JSON syntax; a validator such as the Rich Results Test points at the error."),
            f("structured_data.jsonld", "JSON-LD", "structured_data LIKE '%JSON-LD%'"),
            f("structured_data.microdata", "Microdata", "structured_data LIKE '%Microdata%'"),
            f("structured_data.rdfa", "RDFa", "structured_data LIKE '%RDFa%'"),
        ],
    },
    View {
        id: "social",
        title: "Social Tags",
        section: Section::OnPage,
        scope: PAGES,
        columns: &["address", "title", "indexability"],
        filters: &[
            f("social.all", "All", "1"),
            p("social.no_open_graph", "Missing Open Graph", "has_open_graph = 0", Notice, Low,
              "Pages without Open Graph tags, so links shared on social networks show whatever the network guesses.",
              "Add og:title, og:description, og:image and og:url."),
            p("social.no_twitter_card", "Missing Twitter Card", "has_twitter_card = 0", Notice, Low,
              "Pages without a twitter:card tag.",
              "Add twitter:card (usually summary_large_image)."),
        ],
    },
    // ---- Indexing -------------------------------------------------------------
    View {
        id: "canonicals",
        title: "Canonicals",
        section: Section::Indexing,
        scope: PAGES,
        columns: &["address", "canonical", "indexability", "indexability_status"],
        filters: &[
            f("canonicals.all", "All", "1"),
            f("canonicals.contains", "Contains Canonical", "canonical IS NOT NULL"),
            f("canonicals.self", "Self Referencing", "canonical = url"),
            p("canonicals.canonicalised", "Canonicalised", "canonical IS NOT NULL AND canonical <> url", Warning, High,
              "Pages whose canonical names a different URL, asking not to be indexed themselves.",
              "Make sure that is intended, and link internally to the canonical URL."),
            p("canonicals.missing", "Missing", "canonical IS NULL", Warning, Medium,
              "Pages without a canonical tag, leaving search engines to guess among duplicates.",
              "Add a self-referencing canonical to indexable pages."),
            p("canonicals.non_indexable", "Non-Indexable Canonical", "EXISTS (SELECT 1 FROM urls c WHERE c.url = urls.canonical AND c.url <> urls.url AND c.indexable = 0)", Error, High,
              "Canonicals pointing at a redirect, an error or a noindex page; search engines may ignore them.",
              "Point each canonical at an indexable page that answers 200."),
            p("canonicals.other_domain", "Canonical to Another Domain", concat!("canonical IS NOT NULL AND ", origin_of!("canonical"), " <> ", origin_of!("url")), Warning, High,
              "Pages handing their search presence to a different host or protocol.",
              "Point canonicals at the same site unless content really is syndicated."),
            p("canonicals.chain", "Canonical Chain", "canonical IS NOT NULL AND canonical <> url AND EXISTS (SELECT 1 FROM urls c WHERE c.url = urls.canonical AND c.canonical IS NOT NULL AND c.canonical <> c.url)", Warning, Medium,
              "Canonicals pointing at a page that itself names another canonical.",
              "Point every page straight at the final canonical URL."),
            p("canonicals.relative", "Canonical Is Relative", "canonical_relative = 1", Warning, Low,
              "Canonical tags written as relative URLs, which are resolved inconsistently.",
              "Write the canonical as a full absolute URL."),
        ],
    },
    View {
        id: "directives",
        title: "Directives",
        section: Section::Indexing,
        scope: PAGES,
        columns: &["address", "meta_robots", "x_robots", "indexability", "indexability_status"],
        filters: &[
            f("directives.all", "All", "1"),
            f("directives.index", "Index", concat!(dir!(), " NOT REGEXP '[ ,](noindex|none)[ ,]'")),
            p("directives.noindex", "Noindex", has_dir!("(noindex|none)"), Warning, High,
              "Pages asking search engines to leave them out of results.",
              "Check each one really should stay out of search."),
            p("directives.noindex_linked", "Noindex Page Linked Internally", concat!(has_dir!("(noindex|none)"), " AND EXISTS (SELECT 1 FROM inlinks i WHERE i.url = urls.url AND i.anchors > 0)"), Notice, Low,
              "Noindex pages that other pages still link to, spending links on pages that will not rank.",
              "Remove the links, or make the page indexable if it should rank."),
            f("directives.follow", "Follow", concat!(dir!(), " NOT REGEXP '[ ,](nofollow|none)[ ,]'")),
            p("directives.nofollow", "Nofollow", has_dir!("(nofollow|none)"), Warning, High,
              "Pages asking crawlers to ignore every link on them.",
              "Remove nofollow from pages whose links should be crawled."),
            f("directives.none", "None", has_dir!("none")),
            f("directives.noarchive", "NoArchive", has_dir!("noarchive")),
            f("directives.nosnippet", "NoSnippet", has_dir!("nosnippet")),
            f("directives.max_snippet", "Max-Snippet", has_dir!("max-snippet")),
            f("directives.max_image_preview", "Max-Image-Preview", has_dir!("max-image-preview")),
            f("directives.max_video_preview", "Max-Video-Preview", has_dir!("max-video-preview")),
            f("directives.noimageindex", "NoImageIndex", has_dir!("noimageindex")),
            f("directives.unavailable_after", "Unavailable_After", has_dir!("unavailable_after")),
        ],
    },
    View {
        id: "sitemaps",
        title: "Sitemaps",
        section: Section::Indexing,
        scope: "internal = 1 AND (in_sitemap = 1 OR resource_type = 'html')",
        columns: &["address", "in_sitemap", "sitemap_lastmod", "status_code", "indexability", "indexability_status", "inlinks", "found_via"],
        filters: &[
            f("sitemaps.all", "All", "1"),
            f("sitemaps.in_sitemap", "URLs in Sitemap", "in_sitemap = 1"),
            p("sitemaps.not_in_sitemap", "URLs Not in Sitemap", "in_sitemap = 0 AND indexable = 1 AND resource_type = 'html' AND EXISTS (SELECT 1 FROM urls s WHERE s.in_sitemap = 1)", Notice, Medium,
              "Indexable pages the XML sitemap leaves out.",
              "List every page you want found (Bulk Export > Sitemaps > XML Sitemap can build one)."),
            p("sitemaps.orphans", "Orphan URLs", "found_via = 'sitemap'", Warning, Medium,
              "Sitemap URLs that no crawled page links to. They were crawled only because the sitemap lists them.",
              "Link to them from relevant pages, or drop them from the sitemap if retired."),
            p("sitemaps.non_indexable", "Non-Indexable URLs in Sitemap", "in_sitemap = 1 AND indexable = 0 AND status = 200", Error, Medium,
              "Sitemap entries that are noindex or canonicalised elsewhere. A sitemap should list indexable URLs only.",
              "Remove them from the sitemap or make them indexable."),
            p("sitemaps.redirects", "Redirects in Sitemap", "in_sitemap = 1 AND status BETWEEN 300 AND 399", Warning, Medium,
              "Sitemap entries that redirect.",
              "List the final destination URL instead."),
            p("sitemaps.errors", "Errors in Sitemap", "in_sitemap = 1 AND (status >= 400 OR (status IS NULL AND robots_blocked = 0))", Error, High,
              "Sitemap entries that answer with an error or not at all.",
              "Remove dead URLs from the sitemap, or restore the pages."),
            p("sitemaps.lastmod_missing", "Missing Lastmod", "in_sitemap = 1 AND sitemap_lastmod IS NULL", Notice, Low,
              "Sitemap entries without a lastmod date, which search engines use to decide what to recrawl.",
              "Write each page's real last-modified date into the sitemap."),
            p("sitemaps.lastmod_future", "Lastmod in the Future", "in_sitemap = 1 AND sitemap_lastmod > date('now', '+1 day')", Warning, Low,
              "Sitemap entries dated in the future, which makes the date worthless as a signal.",
              "Write the real modified date."),
            p("sitemaps.lastmod_malformed", "Malformed Lastmod", "in_sitemap = 1 AND sitemap_lastmod IS NOT NULL AND sitemap_lastmod NOT REGEXP '^[0-9]{4}-[0-9]{2}-[0-9]{2}([Tt ][0-9:.]+([Zz]|[+-][0-9]{2}:?[0-9]{2})?)?$'", Warning, Low,
              "Sitemap dates not in the W3C format search engines expect.",
              "Write lastmod as YYYY-MM-DD or a full date and time with a time zone."),
        ],
    },
    View {
        id: "hreflang",
        title: "Hreflang",
        section: Section::Indexing,
        scope: PAGES,
        columns: &["address", "hreflang", "indexability"],
        filters: &[
            f("hreflang.all", "All", "1"),
            f("hreflang.contains", "Contains Hreflang", "hreflang_links > 0"),
            p("hreflang.missing", "Missing", "hreflang_links = 0 AND EXISTS (SELECT 1 FROM urls h WHERE h.hreflang_links > 0)", Warning, Medium,
              "Pages without hreflang on a site that uses it elsewhere.",
              "Add hreflang for every language version, including one for the page itself."),
            p("hreflang.invalid_code", "Invalid Language Code", "EXISTS (SELECT 1 FROM links l WHERE l.from_url = urls.url AND l.kind = 'hreflang' AND l.anchor NOT REGEXP '^(?i)([a-z]{2,3}(-[a-z]{4})?(-([a-z]{2}|[0-9]{3}))?|x-default)$')", Error, Medium,
              "hreflang values that are not a valid language (and optional region) code, such as en, en-GB or x-default.",
              "Use ISO 639-1 language codes, optionally with an ISO 3166-1 region."),
            p("hreflang.no_self", "Missing Self Reference", "hreflang_links > 0 AND NOT EXISTS (SELECT 1 FROM links l WHERE l.from_url = urls.url AND l.kind = 'hreflang' AND l.to_url = urls.url)", Warning, Low,
              "Pages whose hreflang set does not include the page itself.",
              "Add an hreflang entry pointing at the page itself."),
            p("hreflang.no_return", "Missing Return Links", "EXISTS (SELECT 1 FROM links l JOIN urls t ON t.url = l.to_url AND t.status = 200 WHERE l.from_url = urls.url AND l.kind = 'hreflang' AND l.to_url <> urls.url AND NOT EXISTS (SELECT 1 FROM links r WHERE r.from_url = l.to_url AND r.kind = 'hreflang' AND r.to_url = urls.url))", Error, Medium,
              "Pages naming a language version that does not name them back. hreflang must be mutual to work.",
              "Add the missing hreflang link on each alternate page."),
        ],
    },
    View {
        id: "javascript",
        title: "JavaScript",
        section: Section::Indexing,
        scope: "internal = 1 AND resource_type = 'html'",
        columns: &["address", "status_code", "rendered", "js_links", "js_words", "render_error", "word_count", "indexability"],
        filters: &[
            f("javascript.all", "All", "1"),
            f("javascript.rendered", "Rendered", "rendered = 1"),
            p("javascript.links", "Contains JavaScript Links", "js_only_links > 0", Warning, Low,
              "Pages whose links only appear after JavaScript runs. Crawlers that do not render miss them.",
              "Put important links in the HTML the server sends."),
            p("javascript.content", "Contains JavaScript Content", "js_only_words > 50", Warning, Low,
              "Pages where JavaScript adds a lot of visible text that the server's HTML does not contain.",
              "Render important content on the server."),
            p("javascript.failed", "Rendering Failed", "render_error IS NOT NULL", Warning, Medium,
              "Pages the browser could not render in time; the raw HTML was used instead.",
              "Raise the wait time in Configuration > Rendering, or check the page loads without errors."),
        ],
    },
    // ---- Linking ----------------------------------------------------------------
    View {
        id: "links",
        title: "Links",
        section: Section::Linking,
        scope: PAGES,
        columns: &["address", "depth", "inlinks", "outlinks", "ext_outlinks", "broken_links", "link_score", "indexability"],
        filters: &[
            f("links.all", "All", "1"),
            p("links.to_broken", "Internal Outlinks to Broken Pages", concat!("url IN (SELECT l.from_url FROM links l WHERE l.to_url IN (", broken!(), "))"), Error, High,
              "Pages with at least one link or resource that leads to an error or no response.",
              "Open the Outlinks tab, fix or remove the broken targets."),
            p("links.to_redirects", "Internal Outlinks to Redirects", "url IN (SELECT l.from_url FROM links l WHERE l.kind = 'a' AND l.to_url IN (SELECT t.url FROM urls t WHERE t.status BETWEEN 300 AND 399))", Warning, Low,
              "Pages linking to URLs that redirect, which adds a hop for every click.",
              "Update the links to point at the final address."),
            p("links.no_internal_outlinks", "Pages Without Internal Outlinks", "out_internal = 0", Warning, Low,
              "Pages that link nowhere else on the site, a dead end for visitors and crawlers.",
              "Link to related pages, or check whether links are built with JavaScript."),
            p("links.internal_nofollow", "Internal Nofollow Outlinks", "out_internal_nofollow > 0", Warning, Low,
              "Pages with nofollow links to other pages on the same site.",
              "Remove nofollow from internal links you want crawled."),
            p("links.no_anchor_text", "Internal Outlinks With No Anchor Text", "out_no_anchor > 0", Notice, Low,
              "Pages with internal links that have no text or image alt to describe the target.",
              "Give links descriptive text, or alt text on linked images."),
            p("links.non_descriptive", "Non-Descriptive Anchor Text", "out_generic_anchor > 0", Notice, Low,
              "Pages with links such as \"click here\" or \"read more\", which say nothing about the target.",
              "Use link text that names what the linked page is about."),
            p("links.high_external", "High External Outlinks", "out_external > 100", Warning, Low,
              "Pages with more than 100 links to other sites.",
              "Check they are useful and not spam, for example in comments."),
            p("links.high_internal", "High Internal Outlinks", "out_internal > 1000", Warning, Low,
              "Pages with more than 1,000 internal links, so each counts for little.",
              "Trim navigation and lists to what helps people."),
            p("links.high_depth", "High Crawl Depth", "depth > 4", Notice, Low,
              "Pages more than four clicks from the start page get crawled and found less.",
              "Link to important pages from higher up in the site."),
            p("links.one_inlink", "Only One Inlink", "EXISTS (SELECT 1 FROM inlinks i WHERE i.url = urls.url AND i.anchors = 1)", Notice, Low,
              "Pages only one internal link points to, which makes them easy to lose.",
              "Link to them from more related pages."),
            p("links.nofollow_inlinks_only", "Nofollow Inlinks Only", "EXISTS (SELECT 1 FROM inlinks i WHERE i.url = urls.url AND i.nofollow_anchors > 0 AND i.nofollow_anchors = i.anchors)", Warning, Low,
              "Pages that every internal link to them marks nofollow.",
              "Add followed links to pages you want crawled."),
        ],
    },
];

/// Index of a (view, filter) pair in a flat list of all filters.
pub fn flat_index(view: usize, filter: usize) -> usize {
    VIEWS[..view].iter().map(|v| v.filters.len()).sum::<usize>() + filter
}

/// Full WHERE condition for a view's filter.
pub fn condition(view: usize, filter: usize) -> String {
    format!("({}) AND ({})", VIEWS[view].scope, VIEWS[view].filters[filter].sql)
}

/// "Page Titles: Missing"
pub fn problem_name(view: usize, filter: usize) -> String {
    format!("{}: {}", VIEWS[view].title, VIEWS[view].filters[filter].label)
}

/// Every filter that is a problem, as (view, filter, problem).
pub fn problems() -> impl Iterator<Item = (usize, usize, &'static Problem)> {
    VIEWS.iter().enumerate().flat_map(|(vi, v)| {
        v.filters.iter().enumerate().filter_map(move |(fi, f)| f.problem.as_ref().map(|p| (vi, fi, p)))
    })
}

/// Find a filter by its slug.
pub fn by_slug(slug: &str) -> Option<(usize, usize)> {
    let slug = RENAMED.iter().find(|(old, _)| *old == slug).map_or(slug, |(_, new)| new);
    VIEWS.iter().enumerate().find_map(|(vi, v)| v.filters.iter().position(|f| f.slug == slug).map(|fi| (vi, fi)))
}

/// Slugs from before the catalogue used the usual report names.
const RENAMED: &[(&str, &str)] = &[
    ("pages.all", "internal.all"),
    ("pages.html", "internal.html"),
    ("pages.js", "internal.javascript"),
    ("pages.css", "internal.css"),
    ("pages.images", "internal.images"),
    ("pages.pdf", "internal.pdf"),
    ("pages.other", "internal.other"),
    ("external.files", "external.resources"),
    ("status.all", "response_codes.all"),
    ("status.ok", "response_codes.success"),
    ("status.redirect", "response_codes.redirection"),
    ("status.broken", "response_codes.client_error"),
    ("status.server", "response_codes.server_error"),
    ("status.none", "response_codes.no_response"),
    ("status.blocked", "response_codes.blocked"),
    ("status.errors", "response_codes.errors"),
    ("url.long", "url.over_115"),
    ("url.slashes", "url.multiple_slashes"),
    ("url.spaces", "url.space"),
    ("url.search", "url.internal_search"),
    ("security.mixed", "security.mixed_content"),
    ("security.hsts", "security.hsts_missing"),
    ("security.csp", "security.csp_missing"),
    ("titles.all", "page_titles.all"),
    ("titles.missing", "page_titles.missing"),
    ("titles.shared", "page_titles.duplicate"),
    ("titles.long", "page_titles.over_60"),
    ("titles.short", "page_titles.below_30"),
    ("titles.wide", "page_titles.over_580px"),
    ("titles.narrow", "page_titles.below_200px"),
    ("titles.same_h1", "page_titles.same_as_h1"),
    ("descriptions.all", "meta_description.all"),
    ("descriptions.missing", "meta_description.missing"),
    ("descriptions.shared", "meta_description.duplicate"),
    ("descriptions.long", "meta_description.over_155"),
    ("descriptions.short", "meta_description.below_70"),
    ("descriptions.wide", "meta_description.over_990px"),
    ("descriptions.narrow", "meta_description.below_400px"),
    ("h1.shared", "h1.duplicate"),
    ("h1.long", "h1.over_70"),
    ("h2.shared", "h2.duplicate"),
    ("h2.long", "h2.over_70"),
    ("content.duplicate", "content.exact_duplicates"),
    ("content.thin", "content.low_content"),
    ("content.soft404", "content.soft_404"),
    ("images.heavy", "images.over_100kb"),
    ("images.no_alt", "images.missing_alt"),
    ("images.long_alt", "images.alt_over_100"),
    ("structured.all", "structured_data.all"),
    ("structured.present", "structured_data.contains"),
    ("structured.missing", "structured_data.missing"),
    ("structured.jsonld", "structured_data.jsonld"),
    ("structured.microdata", "structured_data.microdata"),
    ("structured.rdfa", "structured_data.rdfa"),
    ("canonicals.present", "canonicals.contains"),
    ("canonicals.elsewhere", "canonicals.canonicalised"),
    ("canonicals.other_site", "canonicals.other_domain"),
    ("canonicals.bad_target", "canonicals.non_indexable"),
    ("directives.max_image", "directives.max_image_preview"),
    ("directives.max_video", "directives.max_video_preview"),
    ("directives.unavailable", "directives.unavailable_after"),
    ("sitemaps.listed", "sitemaps.in_sitemap"),
    ("sitemaps.unlisted", "sitemaps.not_in_sitemap"),
    ("sitemaps.not_indexable", "sitemaps.non_indexable"),
    ("hreflang.present", "hreflang.contains"),
    ("rendering.all", "javascript.all"),
    ("rendering.rendered", "javascript.rendered"),
    ("rendering.failed", "javascript.failed"),
    ("linking.all", "links.all"),
    ("linking.broken", "links.to_broken"),
    ("linking.to_redirects", "links.to_redirects"),
    ("linking.dead_end", "links.no_internal_outlinks"),
    ("linking.nofollow_out", "links.internal_nofollow"),
    ("linking.no_text", "links.no_anchor_text"),
    ("linking.vague_text", "links.non_descriptive"),
    ("linking.many_external", "links.high_external"),
    ("linking.many_internal", "links.high_internal"),
    ("linking.deep", "links.high_depth"),
    ("linking.nofollow_only", "links.nofollow_inlinks_only"),
];

/// One problem found on one URL. `id` stays the same across crawls of the
/// same site while the problem persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    pub id: String,
    pub check: &'static str,
    pub severity: &'static str,
    pub impact: &'static str,
    pub problem: String,
    pub url: String,
    pub how_to_fix: &'static str,
}

/// Stable identity of a problem on a URL.
pub fn finding_id(slug: &str, url: &str) -> String {
    format!("{:016x}", crate::norm::fnv1a64(format!("{slug}\n{url}").as_bytes()))
}

/// Every problem in a crawl at `min` severity or worse, worst first.
pub fn findings(store: &crate::store::Store, min: Severity) -> anyhow::Result<Vec<Finding>> {
    let mut out = vec![];
    for (vi, fi, problem) in problems() {
        if problem.severity > min {
            continue;
        }
        let filter = &VIEWS[vi].filters[fi];
        let urls = store.table(&format!("SELECT url FROM urls WHERE {} ORDER BY url", condition(vi, fi)))?;
        for row in urls.rows {
            let url = crate::store::value_text(&row[0]);
            out.push(Finding {
                id: finding_id(filter.slug, &url),
                check: filter.slug,
                severity: problem.severity.label(),
                impact: problem.impact.label(),
                problem: problem_name(vi, fi),
                how_to_fix: problem.fix,
                url,
            });
        }
    }
    out.sort_by(|a, b| {
        let rank = |f: &Finding| Severity::parse(f.severity).unwrap_or(Severity::Notice);
        rank(a).cmp(&rank(b)).then_with(|| a.check.cmp(b.check)).then_with(|| a.url.cmp(&b.url))
    });
    Ok(out)
}

/// The check catalogue as a Markdown reference page (docs/CHECKS.md).
pub fn reference_markdown() -> String {
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let count = |sev: Severity| problems().filter(|(.., p)| p.severity == sev).count();
    let mut out = String::new();
    out.push_str("# ShoutingMonkey check reference\n\n");
    out.push_str(&format!(
        "Every technical SEO check ShoutingMonkey runs on a crawl: {} issues ({} errors, {} warnings, {} notices) in {} tabs. \
         Each has a stable check id, used in reports, `--fail-on` and Crawl Compare.\n\n\
         This page is generated from the code (`shoutingmonkey-cli checks --markdown`), so it always matches what the app reports.\n",
        problems().count(),
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Notice),
        VIEWS.len()
    ));
    for section in Section::ALL {
        out.push_str(&format!("\n## {}\n", section.label()));
        for view in VIEWS.iter().filter(|v| v.section == section) {
            out.push_str(&format!("\n### {}\n\n", view.title));
            let issues: Vec<&Filter> = view.filters.iter().filter(|f| f.problem.is_some()).collect();
            if !issues.is_empty() {
                out.push_str("| Issue | Type | Priority | What it means | How to fix | Check id |\n|---|---|---|---|---|---|\n");
                for f in issues {
                    let p = f.problem.as_ref().expect("filtered to problems");
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} | `{}` |\n",
                        cell(f.label),
                        p.severity.label(),
                        p.impact.label(),
                        cell(p.about),
                        cell(p.fix),
                        f.slug
                    ));
                }
                out.push('\n');
            }
            let groups: Vec<&str> = view.filters.iter().filter(|f| f.problem.is_none()).map(|f| f.label).collect();
            out.push_str(&format!("Other filters: {}.\n", groups.join(", ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/CHECKS.md matches the catalogue. Regenerate it with
    /// `shoutingmonkey-cli checks --markdown > docs/CHECKS.md`.
    #[test]
    fn check_reference_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/CHECKS.md");
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert!(on_disk == reference_markdown(), "docs/CHECKS.md is out of date: run `shoutingmonkey-cli checks --markdown > docs/CHECKS.md`");
    }

    #[test]
    fn catalogue_is_consistent() {
        let mut slugs = std::collections::HashSet::new();
        for v in VIEWS {
            assert!(v.filters[0].problem.is_none(), "{}: first filter shows everything", v.title);
            assert_eq!(v.columns[0], "address", "{}", v.title);
            for c in v.columns {
                col(c);
            }
            for f in v.filters {
                assert!(slugs.insert(f.slug), "duplicate slug {}", f.slug);
                assert!(f.slug.starts_with(&format!("{}.", v.id)), "{} not under {}", f.slug, v.id);
            }
        }
        assert!(problems().count() > 50);
        assert_eq!(by_slug("page_titles.missing").map(|(v, _)| VIEWS[v].id), Some("page_titles"));
        assert_eq!(finding_id("a", "b"), finding_id("a", "b"));
        assert_ne!(finding_id("a", "b"), finding_id("a", "c"));
    }

    #[test]
    fn every_filter_is_valid_sql() {
        let dir = std::env::temp_dir().join(format!("sm-checks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        let store = crate::store::Store::open(&path).unwrap();
        for (vi, v) in VIEWS.iter().enumerate() {
            for fi in 0..v.filters.len() {
                store
                    .table(&format!("SELECT COUNT(*) FROM urls WHERE {}", condition(vi, fi)))
                    .unwrap_or_else(|e| panic!("{}: {e}", v.filters[fi].slug));
            }
        }
        assert!(findings(&store, Severity::Notice).unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
