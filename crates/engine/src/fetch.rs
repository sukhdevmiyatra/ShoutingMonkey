use std::time::{Duration, Instant};

use encoding_rs::Encoding;
use reqwest::{Client, StatusCode, header};
use url::Url;

use crate::config::CrawlConfig;

/// A completed HTTP exchange (any status). Redirects are never followed by
/// the page client: the crawler records them and queues `location` itself.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub x_robots_tag: Option<String>,
    /// All response headers, one "name: value" per line.
    pub headers: String,
    /// Bytes received, or the Content-Length when the body was not read.
    pub size: Option<u64>,
    pub body: Option<String>,
    /// True when the body hit `max_response_bytes` and was cut short.
    pub truncated: bool,
    pub elapsed_ms: u64,
}

/// HTTP client for crawled pages. `follow_redirects` is only for auxiliary
/// fetches (robots.txt, sitemaps) whose final content is all that matters.
pub fn build_client(cfg: &CrawlConfig, follow_redirects: bool) -> reqwest::Result<Client> {
    let policy = if follow_redirects {
        reqwest::redirect::Policy::limited(cfg.max_redirects.max(1) as usize)
    } else {
        reqwest::redirect::Policy::none()
    };
    Client::builder()
        .user_agent(&cfg.user_agent)
        .redirect(policy)
        .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
        .connect_timeout(Duration::from_secs(cfg.timeout_secs.clamp(1, 15)))
        .pool_max_idle_per_host(cfg.concurrency.max(1))
        .build()
}

fn header_str(h: &header::HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    let vals: Vec<&str> = h.get_all(name).iter().filter_map(|v| v.to_str().ok()).collect();
    (!vals.is_empty()).then(|| vals.join(", "))
}

/// The `charset=` parameter of a Content-Type value.
fn charset_param(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| v.trim().trim_matches(['"', '\'']))
    })
}

/// `<meta charset=...>` or `<meta http-equiv content="...; charset=...">`
/// within the first 1 KiB, as browsers pre-scan for it.
fn sniff_meta_charset(head: &[u8]) -> Option<&'static Encoding> {
    let head = String::from_utf8_lossy(&head[..head.len().min(1024)]).to_ascii_lowercase();
    let at = head.find("charset=")? + "charset=".len();
    let label: String = head[at..]
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
        .collect();
    Encoding::for_label(label.as_bytes())
}

/// Decode a body: BOM, then Content-Type charset, then meta pre-scan,
/// defaulting to UTF-8. Invalid sequences become U+FFFD.
pub fn decode_body(bytes: &[u8], content_type: Option<&str>) -> String {
    let declared = content_type
        .and_then(charset_param)
        .and_then(|l| Encoding::for_label(l.as_bytes()))
        .or_else(|| sniff_meta_charset(bytes))
        .unwrap_or(encoding_rs::UTF_8);
    // `decode` lets a BOM override the declared encoding.
    declared.decode(bytes).0.into_owned()
}

/// Human-readable error with its causes (reqwest's top-level message alone
/// is just "error sending request for url").
pub fn describe_error(e: &reqwest::Error) -> String {
    let mut msg = String::from(if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "connection failed"
    } else if e.is_redirect() {
        "redirect error"
    } else if e.is_body() || e.is_decode() {
        "error reading response body"
    } else {
        "request failed"
    });
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        let part = s.to_string();
        if !msg.contains(&part) {
            msg.push_str(": ");
            msg.push_str(&part);
        }
        src = s.source();
    }
    msg
}

/// GET `url` once. The body is only downloaded when `want_body` says so for
/// the response's status and content type, and is capped at `max_bytes`.
pub async fn fetch_once(
    client: &Client,
    url: &Url,
    max_bytes: u64,
    want_body: impl Fn(StatusCode, Option<&str>) -> bool,
) -> reqwest::Result<Fetched> {
    let start = Instant::now();
    let mut resp = client.get(url.as_str()).send().await?;
    let status = resp.status();
    let headers = resp.headers();
    let content_type = header_str(headers, header::CONTENT_TYPE);
    let location = header_str(headers, header::LOCATION);
    let x_robots_tag = header_str(headers, "x-robots-tag");
    let all_headers = headers
        .iter()
        .map(|(k, v)| format!("{}: {}", k.as_str(), String::from_utf8_lossy(v.as_bytes())))
        .collect::<Vec<_>>()
        .join("
");

    let (mut size, mut body, mut truncated) = (resp.content_length(), None, false);
    if want_body(status, content_type.as_deref()) {
        let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            let room = cap.saturating_sub(buf.len());
            if chunk.len() > room {
                buf.extend_from_slice(&chunk[..room]);
                truncated = true;
                break;
            }
            buf.extend_from_slice(&chunk);
        }
        size = Some(buf.len() as u64);
        body = Some(decode_body(&buf, content_type.as_deref()));
    }

    Ok(Fetched {
        status: status.as_u16(),
        content_type,
        location,
        x_robots_tag,
        headers: all_headers,
        size,
        body,
        truncated,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// `fetch_once` with retries on network errors, 429 and 5xx, backing off
/// one second per attempt. The last response or error is returned.
pub async fn fetch(
    client: &Client,
    url: &Url,
    max_bytes: u64,
    retries: usize,
    want_body: impl Fn(StatusCode, Option<&str>) -> bool + Copy,
) -> reqwest::Result<Fetched> {
    let mut attempt = 0;
    loop {
        let res = fetch_once(client, url, max_bytes, want_body).await;
        let retryable = match &res {
            Ok(f) => f.status == 429 || f.status >= 500,
            Err(e) => !e.is_builder() && !e.is_redirect(),
        };
        if !retryable || attempt >= retries {
            return res;
        }
        attempt += 1;
        tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_declared_charsets() {
        // "café" in windows-1252
        let bytes = b"caf\xe9";
        assert_eq!(decode_body(bytes, Some("text/html; charset=windows-1252")), "café");
        let html = b"<html><head><meta charset=\"iso-8859-1\"></head>caf\xe9";
        assert!(decode_body(html, Some("text/html")).ends_with("café"));
        assert_eq!(decode_body("café".as_bytes(), None), "café");
    }

    #[test]
    fn charset_param_parsing() {
        assert_eq!(charset_param("text/html; Charset=\"UTF-8\""), Some("UTF-8"));
        assert_eq!(charset_param("text/html"), None);
    }
}
