use quick_xml::Reader;
use quick_xml::events::Event;

/// Locations listed by one sitemap document: page URLs from a `<urlset>`
/// and child sitemaps from a `<sitemapindex>`.
#[derive(Debug, Default, PartialEq)]
pub struct SitemapDoc {
    pub urls: Vec<String>,
    /// `<lastmod>` of each entry in `urls`, when given.
    pub lastmods: Vec<Option<String>>,
    pub sitemaps: Vec<String>,
    /// The XML could not be read to the end.
    pub malformed: bool,
}

/// Parse a sitemap or sitemap index. Malformed XML yields whatever was
/// read before the error, with `malformed` set.
pub fn parse(xml: &str) -> SitemapDoc {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut doc = SitemapDoc::default();
    // The element enclosing the current <loc>: "url" or "sitemap".
    let mut parent: Option<Vec<u8>> = None;
    // The text element being read: "loc" or "lastmod".
    let mut field: Option<&'static str> = None;
    let (mut loc, mut lastmod) = (String::new(), String::new());

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"url" | b"sitemap" => {
                    parent = Some(e.local_name().as_ref().to_vec());
                    loc.clear();
                    lastmod.clear();
                }
                b"loc" => field = Some("loc"),
                b"lastmod" => field = Some("lastmod"),
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if let (Some(f), Ok(text)) = (field, t.unescape()) {
                    if f == "loc" { loc.push_str(&text) } else { lastmod.push_str(&text) }
                }
            }
            Ok(Event::CData(t)) => {
                let text = String::from_utf8_lossy(&t);
                match field {
                    Some("loc") => loc.push_str(&text),
                    Some(_) => lastmod.push_str(&text),
                    None => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"loc" | b"lastmod" => field = None,
                b"url" | b"sitemap" => {
                    let v = loc.trim().to_string();
                    if !v.is_empty() {
                        match parent.as_deref() {
                            Some(b"sitemap") => doc.sitemaps.push(v),
                            _ => {
                                doc.urls.push(v);
                                let m = lastmod.trim();
                                doc.lastmods.push((!m.is_empty()).then(|| m.to_string()));
                            }
                        }
                    }
                    parent = None;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => {
                doc.malformed = true;
                break;
            }
            _ => {}
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlset() {
        let d = parse(
            r#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <url><loc> https://example.com/a?x=1&amp;y=2 </loc><lastmod>2024-01-01</lastmod></url>
            <url><loc><![CDATA[https://example.com/b]]></loc></url></urlset>"#,
        );
        assert_eq!(d.urls, vec!["https://example.com/a?x=1&y=2", "https://example.com/b"]);
        assert_eq!(d.lastmods, vec![Some("2024-01-01".to_string()), None]);
        assert!(!d.malformed);        assert!(d.sitemaps.is_empty());
    }

    #[test]
    fn index() {
        let d = parse(
            r#"<sitemapindex><sitemap><loc>https://example.com/s1.xml</loc></sitemap></sitemapindex>"#,
        );
        assert_eq!(d.sitemaps, vec!["https://example.com/s1.xml"]);
        assert!(d.urls.is_empty());
    }
}
