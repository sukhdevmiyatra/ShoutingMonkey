use url::Url;

/// Canonical crawl identity of a URL: absolute, lowercased scheme/host,
/// no fragment, default ports stripped (the `url` crate already normalizes
/// parsing, so stripping the fragment and reusing `as_str` is enough).
pub fn key(u: &Url) -> String {
    let mut u = u.clone();
    u.set_fragment(None);
    u.as_str().to_string()
}

/// Parse + canonicalize anything the parser extracted (hrefs are often
/// relative, whitespace-y or empty). Returns the normalized key form.
pub fn resolve_key(base: &Url, href: &str) -> Option<String> {
    let href = href.trim();
    // Empty hrefs and pure fragments point back at the page itself.
    if href.is_empty() || href.starts_with('#') {
        return None;
    }
    // javascript:, mailto:, tel:, data:, etc. are rejected by the scheme check.
    let u = base.join(href).ok()?;
    let fetchable = matches!(u.scheme(), "http" | "https") && u.host_str().is_some();
    fetchable.then(|| key(&u))
}

/// `scheme://host[:port]` of a URL: the scope robots.txt applies to.
pub fn origin(u: &Url) -> String {
    u.origin().ascii_serialization()
}

/// Lowercased host without a trailing dot ("" for host-less URLs).
pub fn host(u: &Url) -> String {
    u.host_str().unwrap_or("").trim_end_matches('.').to_ascii_lowercase()
}

/// Stable FNV-1a 64-bit hash (process-independent, unlike DefaultHasher).
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Duplicate-content hash over the page's visible text.
pub fn dup_hash(text: &str) -> u64 {
    fnv1a64(normalize_ws(text).as_bytes())
}

pub fn word_count(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// A host without a trailing dot or a leading `www.`, lowercased.
fn bare_host(h: &str) -> String {
    let h = h.to_ascii_lowercase();
    let h = h.trim_end_matches('.');
    h.strip_prefix("www.").unwrap_or(h).to_string()
}

/// Whether two hosts are one site's `www.` and bare forms (or the same).
pub fn host_twins(a: &str, b: &str) -> bool {
    bare_host(a) == bare_host(b)
}

/// The registrable domain a host belongs to, from the public suffix list:
/// `example.co.uk` for `shop.example.co.uk`. IP addresses and single-label
/// hosts such as `localhost` are their own domain.
pub fn root_domain(host: &str) -> String {
    let h = host.to_ascii_lowercase();
    let h = h.trim_end_matches('.');
    if !h.contains('.') || h.parse::<std::net::IpAddr>().is_ok() {
        return h.to_string();
    }
    psl::domain_str(h).unwrap_or(h).to_string()
}

/// True when `url_host` sits on the same site as `seed_host`. A host and its
/// `www.` form are always one site, whichever was entered; with subdomains
/// included, every host under the same registrable domain is, so starting
/// at `blog.example.com` also covers `www.example.com` and `shop.example.com`.
pub fn same_site(seed_host: &str, url_host: &str, include_subdomains: bool) -> bool {
    host_twins(seed_host, url_host) || (include_subdomains && root_domain(seed_host) == root_domain(url_host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomains_share_the_root_domain() {
        assert_eq!(root_domain("shop.example.co.uk"), "example.co.uk");
        assert_eq!(root_domain("WWW.Example.com."), "example.com");
        assert_eq!(root_domain("127.0.0.1"), "127.0.0.1");
        assert_eq!(root_domain("localhost"), "localhost");
        assert!(same_site("blog.example.com", "www.example.com", true), "a subdomain start covers the whole domain");
        assert!(same_site("blog.example.com", "shop.example.com", true));
        assert!(!same_site("blog.example.com", "shop.example.com", false));
        assert!(same_site("example.co.uk", "shop.example.co.uk", true));
        assert!(!same_site("a.co.uk", "b.co.uk", true), "a public suffix is not a site");
        assert!(!same_site("user1.github.io", "user2.github.io", true), "nor is a hosting suffix");
        assert!(!same_site("127.0.0.1", "127.0.0.2", true));
    }

    #[test]
    fn www_and_bare_host_are_one_site() {
        assert!(same_site("example.com", "www.example.com", false));
        assert!(same_site("www.example.com", "example.com", false));
        assert!(same_site("WWW.Example.com.", "example.com", false));
        assert!(!same_site("example.com", "blog.example.com", false));
        assert!(same_site("www.example.com", "blog.example.com", true));
        assert!(!same_site("example.com", "notexample.com", true));
    }

    #[test]
    fn strips_fragments_and_normalizes() {
        let base = Url::parse("http://Example.COM:80/a/b").unwrap();
        assert_eq!(key(&base), "http://example.com/a/b");
        let with_frag = Url::parse("https://example.com/page#section").unwrap();
        assert_eq!(key(&with_frag), "https://example.com/page");
    }

    #[test]
    fn resolves_relative() {
        let base = Url::parse("https://example.com/dir/page.html").unwrap();
        assert_eq!(
            resolve_key(&base, "/x").as_deref(),
            Some("https://example.com/x")
        );
        assert_eq!(
            resolve_key(&base, "other.html").as_deref(),
            Some("https://example.com/dir/other.html")
        );
        assert_eq!(resolve_key(&base, "#frag"), None);
        assert_eq!(resolve_key(&base, "javascript:void(0)"), None);
        assert_eq!(resolve_key(&base, "mailto:a@b.c"), None);
        assert_eq!(resolve_key(&base, ""), None);
        assert_eq!(
            resolve_key(&base, "?q=1").as_deref(),
            Some("https://example.com/dir/page.html?q=1")
        );
        assert_eq!(
            resolve_key(&base, "//cdn.example.com/lib.js").as_deref(),
            Some("https://cdn.example.com/lib.js")
        );
    }

    #[test]
    fn origin_keeps_scheme_and_port() {
        let u = Url::parse("http://Example.com:8080/a?b").unwrap();
        assert_eq!(origin(&u), "http://example.com:8080");
        assert_eq!(origin(&Url::parse("https://example.com/").unwrap()), "https://example.com");
    }

    #[test]
    fn same_site_rules() {
        assert!(same_site("example.com", "example.com", false));
        assert!(!same_site("example.com", "shop.example.com", false));
        assert!(same_site("example.com", "shop.example.com", true));
        assert!(!same_site("example.com", "notexample.com", true));
    }

    #[test]
    fn dup_hash_stable() {
        assert_eq!(dup_hash("Hello   world\n\nfoo"), dup_hash("Hello world foo"));
        assert_ne!(dup_hash("a"), dup_hash("b"));
    }
}