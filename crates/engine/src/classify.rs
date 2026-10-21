use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Html,
    JavaScript,
    Css,
    Image,
    Pdf,
    Other,
    Redirect,
    NoResponse,
}

impl ResourceType {
    pub fn slug(self) -> &'static str {
        match self {
            ResourceType::Html => "html",
            ResourceType::JavaScript => "javascript",
            ResourceType::Css => "css",
            ResourceType::Image => "image",
            ResourceType::Pdf => "pdf",
            ResourceType::Other => "other",
            ResourceType::Redirect => "redirect",
            ResourceType::NoResponse => "noresponse",
        }
    }

    pub fn parse(slug: &str) -> Self {
        match slug {
            "html" => Self::Html,
            "javascript" => Self::JavaScript,
            "css" => Self::Css,
            "image" => Self::Image,
            "pdf" => Self::Pdf,
            "redirect" => Self::Redirect,
            "noresponse" => Self::NoResponse,
            _ => Self::Other,
        }
    }
}

const IMG_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "tif", "tiff", "avif", "heic",
];

pub fn path_ext(url: &Url) -> Option<String> {
    let path = url.path();
    let last = path.rsplit('/').next()?;
    if last.is_empty() {
        return None;
    }
    let (stem, ext) = last.rsplit_once('.')?;
    if stem.is_empty() || ext.len() > 8 || ext.contains('/') {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Classify a response. Redirect (`status` in 3xx) and missing response
/// (NoResponse) trump content typing; otherwise content-type wins with the
/// extension as fallback.
pub fn classify(status: Option<u16>, content_type: Option<&str>, url: &Url) -> ResourceType {
    if let Some(s) = status {
        if (300..400).contains(&s) {
            return ResourceType::Redirect;
        }
    } else {
        return ResourceType::NoResponse;
    }

    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    let ct_main = ct.split(';').next().unwrap_or("").trim().to_string();
    
    match ct_main.as_str() {
        "text/html" | "application/xhtml+xml" => ResourceType::Html,
        s if s.contains("javascript") || s == "application/ecmascript" => ResourceType::JavaScript,
        "text/css" => ResourceType::Css,
        s if s.starts_with("image/") => ResourceType::Image,
        "application/pdf" => ResourceType::Pdf,
        _ => {
            // content-type missing or generic: fall back on extension
            match path_ext(url).as_deref() {
                Some("html" | "htm" | "xhtml") => ResourceType::Html,
                Some("js" | "mjs") => ResourceType::JavaScript,
                Some("css") => ResourceType::Css,
                Some(ext) if IMG_EXTS.contains(&ext) => ResourceType::Image,
                Some("pdf") => ResourceType::Pdf,
                _ => ResourceType::Other,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_kinds() {
        let u = Url::parse("https://x.com/a").unwrap();
        assert_eq!(
            classify(Some(200), Some("text/html; charset=utf-8"), &u),
            ResourceType::Html
        );
        assert_eq!(classify(Some(200), None, &u), ResourceType::Other);
        let js = Url::parse("https://x.com/app.js").unwrap();
        assert_eq!(classify(Some(200), None, &js), ResourceType::JavaScript);
        assert_eq!(classify(Some(301), Some("text/html"), &u), ResourceType::Redirect);
        assert_eq!(classify(None, None, &u), ResourceType::NoResponse);
        let img = Url::parse("https://x.com/logo.PNG").unwrap();
        assert_eq!(classify(Some(200), None, &img), ResourceType::Image);
    }
}