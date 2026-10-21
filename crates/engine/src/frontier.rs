use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

/// How the crawl came to a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Via {
    Start,
    #[default]
    Link,
    Redirect,
    MetaRefresh,
    Canonical,
    Hreflang,
    Resource,
    Sitemap,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Via::Start => "start",
            Via::Link => "link",
            Via::Redirect => "redirect",
            Via::MetaRefresh => "meta refresh",
            Via::Canonical => "canonical",
            Via::Hreflang => "hreflang",
            Via::Resource => "resource",
            Via::Sitemap => "sitemap",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub url: String,
    pub depth: u32,
    /// Redirect hops taken to reach this URL. The crawl follows redirects
    /// itself (the HTTP client never does), so it can record every hop and
    /// stop a chain that exceeds the configured limit.
    pub redirect_count: u32,
    #[serde(default)]
    pub via: Via,
}

/// Breadth-first frontier with depth buckets: URLs are queued with their
/// click depth and `pop` always returns from the lowest non-empty bucket, so
/// shallow pages are crawled before deep ones. Each URL is queued once.
#[derive(Debug, Default)]
pub struct Frontier {
    /// Bucket d holds all pending URLs at depth d.
    buckets: Vec<VecDeque<Job>>,
    queued: HashSet<String>,
    visited: HashSet<String>,
}

impl Frontier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns false when the URL was already queued or visited.
    pub fn push(&mut self, url: String, depth: u32, redirect_count: u32) -> bool {
        self.push_via(url, depth, redirect_count, Via::Link)
    }

    /// Queue a URL found a particular way. Returns false when it was already
    /// queued or visited.
    pub fn push_via(&mut self, url: String, depth: u32, redirect_count: u32, via: Via) -> bool {
        if self.queued.contains(&url) || self.visited.contains(&url) {
            return false;
        }
        self.queued.insert(url.clone());
        let d = depth as usize;
        if self.buckets.len() <= d {
            self.buckets.resize(d + 1, VecDeque::new());
        }
        self.buckets[d].push_back(Job { url, depth, redirect_count, via });
        true
    }

    /// Pops from the lowest non-empty depth bucket and marks the URL visited
    /// immediately (so a URL being fetched cannot be re-enqueued meanwhile).
    pub fn pop(&mut self) -> Option<Job> {
        // Lowest non-empty depth bucket first.
        for b in self.buckets.iter_mut() {
            if let Some(j) = b.pop_front() {
                self.queued.remove(&j.url);
                self.visited.insert(j.url.clone());
                return Some(j);
            }
        }
        None
    }

    pub fn is_visited(&self, url: &str) -> bool {
        self.visited.contains(url)
    }

    pub fn queue_len(&self) -> usize {
        self.buckets.iter().map(|b| b.len()).sum()
    }

    pub fn visited_len(&self) -> usize {
        self.visited.len()
    }

    /// Remaining pending queue serialized (for resumability).
    pub fn dump(&self) -> String {
        let jobs: Vec<&Job> = self.buckets.iter().flatten().collect();
        serde_json::to_string(&jobs).unwrap_or_else(|_| "[]".to_string())
    }

    /// All visited identifiers (for resumability).
    pub fn dump_visited(&self) -> String {
        serde_json::to_string(&self.visited.iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string())
    }

    /// Seed from serialized state: pending queue plus visited identifiers.
    pub fn restore(&mut self, pending: &str, visited: &str) {
        if let Ok(jobs) = serde_json::from_str::<Vec<Job>>(pending) {
            for j in jobs {
                self.push_via(j.url, j.depth, j.redirect_count, j.via);
            }
        }
        if let Ok(vs) = serde_json::from_str::<Vec<String>>(visited) {
            for v in vs {
                self.visited.insert(v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicates() {
        let mut f = Frontier::new();
        assert!(f.push("https://a/x".into(), 0, 0));
        assert!(!f.push("https://a/x".into(), 0, 0));
        assert_eq!(f.queue_len(), 1);
        let j = f.pop().unwrap();
        assert_eq!(j.url, "https://a/x");
        // once popped it counts as visited — re-push rejected
        assert!(!f.push("https://a/x".into(), 1, 0));
        assert!(f.pop().is_none());
        assert_eq!(f.visited_len(), 1);
    }

    #[test]
    fn depth_bucket_order_and_dump_restore() {
        let mut f = Frontier::new();
        f.push("https://a/1".into(), 0, 0);
        f.push("https://a/2".into(), 1, 0);
        f.push("https://a/3".into(), 1, 0);
        f.push("https://a/4".into(), 2, 0);
        // All of depth 0, then all of depth 1, then depth 2.
        assert_eq!(f.pop().map(|j| (j.url, j.depth)), Some(("https://a/1".into(), 0)));
        assert_eq!(f.pop().map(|j| j.url), Some("https://a/2".into()));
        assert_eq!(f.pop().map(|j| j.url), Some("https://a/3".into()));
        assert_eq!(f.pop().map(|j| j.depth), Some(2));
        let dump = f.dump();
        assert!(f.pop().is_none());
        let mut r = Frontier::new();
        r.restore(&dump, "[]");
        assert!(r.pop().is_none());
    }

    #[test]
    fn late_deeper_push_still_ordered() {
        let mut f = Frontier::new();
        f.push("https://a/1".into(), 0, 0);
        f.push("https://a/2".into(), 2, 0);
        // A new depth-1 job discovered while depth 2 is pending comes first.
        f.push("https://a/3".into(), 1, 0);
        assert_eq!(f.pop().map(|j| j.depth), Some(0));
        assert_eq!(f.pop().map(|j| j.depth), Some(1));
        assert_eq!(f.pop().map(|j| j.depth), Some(2));
        let dump = f.dump();
        assert!(f.pop().is_none());
        let mut r = Frontier::new();
        r.restore(&dump, "[]");
        assert!(r.pop().is_none());
    }
}