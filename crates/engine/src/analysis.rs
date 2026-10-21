//! Post-crawl analysis: figures that need the whole crawl, worked out once
//! it has finished and written back to the `urls` table.
//!
//! - near duplicates: pages whose main content SimHash is within
//!   `NEAR_DUPLICATE_BITS` bits of another page's
//! - link score: a 0–100 PageRank over internal, followed links

use std::collections::HashMap;

use rusqlite::params;

use crate::store::{Store, value_text};

/// Hashes this many bits apart or fewer count as near duplicates (about 95%
/// similar for 64-bit SimHash).
pub const NEAR_DUPLICATE_BITS: u32 = 3;
/// Pages with less main content than this are too short to compare.
const MIN_WORDS: i64 = 50;
const DAMPING: f64 = 0.85;
const ITERATIONS: usize = 25;

pub fn run(store: &Store) -> anyhow::Result<()> {
    let near = near_duplicates(store)?;
    let score = link_scores(store)?;
    let tx = store.conn().unchecked_transaction()?;
    tx.execute("UPDATE urls SET near_duplicates = NULL, link_score = NULL", [])?;
    {
        let mut set_near = tx.prepare("UPDATE urls SET near_duplicates = ?2 WHERE url = ?1")?;
        for (url, n) in &near {
            set_near.execute(params![url, *n as i64])?;
        }
        let mut set_score = tx.prepare("UPDATE urls SET link_score = ?2 WHERE url = ?1")?;
        for (url, s) in &score {
            set_score.execute(params![url, (s * 10.0).round() / 10.0])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// For every comparable page, how many other pages are near duplicates.
fn near_duplicates(store: &Store) -> anyhow::Result<Vec<(String, usize)>> {
    let t = store.table(&format!(
        "SELECT url, simhash FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200
         AND simhash IS NOT NULL AND word_count >= {MIN_WORDS}"
    ))?;
    let pages: Vec<(String, u64)> = t
        .rows
        .iter()
        .filter_map(|r| Some((value_text(&r[0]), u64::from_str_radix(&value_text(&r[1]), 16).ok()?)))
        .filter(|(_, h)| *h != 0)
        .collect();
    // Hashes within 3 bits agree on at least one of four 16-bit bands, so
    // only pages sharing a band need comparing.
    let mut matches: Vec<std::collections::HashSet<usize>> = vec![Default::default(); pages.len()];
    for band in 0..4 {
        let mut buckets: HashMap<u16, Vec<usize>> = HashMap::new();
        for (i, (_, h)) in pages.iter().enumerate() {
            buckets.entry((h >> (band * 16)) as u16).or_default().push(i);
        }
        for members in buckets.values() {
            for (a, &i) in members.iter().enumerate() {
                for &j in &members[a + 1..] {
                    if (pages[i].1 ^ pages[j].1).count_ones() <= NEAR_DUPLICATE_BITS {
                        matches[i].insert(j);
                        matches[j].insert(i);
                    }
                }
            }
        }
    }
    Ok(pages.into_iter().zip(matches).map(|((url, _), m)| (url, m.len())).collect())
}

/// PageRank over internal followed anchor links between crawled pages,
/// scaled logarithmically to 0–100 (the best-linked page scores 100).
fn link_scores(store: &Store) -> anyhow::Result<Vec<(String, f64)>> {
    let nodes = store.table("SELECT url FROM urls WHERE internal = 1 AND resource_type = 'html' AND status = 200")?;
    let urls: Vec<String> = nodes.rows.iter().map(|r| value_text(&r[0])).collect();
    let n = urls.len();
    if n == 0 {
        return Ok(vec![]);
    }
    let index: HashMap<&str, usize> = urls.iter().enumerate().map(|(i, u)| (u.as_str(), i)).collect();
    let edges = store.table("SELECT DISTINCT from_url, to_url FROM links WHERE kind = 'a' AND nofollow = 0 AND to_internal = 1")?;
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    for r in &edges.rows {
        if let (Some(&a), Some(&b)) = (index.get(value_text(&r[0]).as_str()), index.get(value_text(&r[1]).as_str()))
            && a != b
        {
            out[a].push(b);
        }
    }
    let mut rank = vec![1.0 / n as f64; n];
    for _ in 0..ITERATIONS {
        let dangling: f64 = rank.iter().zip(&out).filter(|(_, o)| o.is_empty()).map(|(r, _)| r).sum();
        let mut next = vec![(1.0 - DAMPING) / n as f64 + DAMPING * dangling / n as f64; n];
        for (i, targets) in out.iter().enumerate() {
            if targets.is_empty() {
                continue;
            }
            let share = DAMPING * rank[i] / targets.len() as f64;
            for &t in targets {
                next[t] += share;
            }
        }
        rank = next;
    }
    let max = rank.iter().cloned().fold(0.0, f64::max);
    let scale = |r: f64| 100.0 * (1.0 + r * n as f64).ln() / (1.0 + max * n as f64).ln();
    Ok(urls.into_iter().zip(rank).map(|(u, r)| (u, if max > 0.0 { scale(r) } else { 0.0 })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::PageData;
    use crate::store::{LinkRecord, UrlRecord};

    #[test]
    fn near_duplicates_and_scores() {
        let dir = std::env::temp_dir().join(format!("sm-analysis-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.db");
        let _ = std::fs::remove_file(&path);
        let mut s = Store::open(&path).unwrap();
        let text = "word ".repeat(80);
        let page = |url: &str, simhash: u64| UrlRecord {
            url: url.into(),
            internal: true,
            status: Some(200),
            resource_type: "html",
            found_via: "link",
            page: Some(PageData { word_count: 80, content_simhash: simhash, title: Some(text.clone()), ..Default::default() }),
            ..Default::default()
        };
        let to = |u: &str| LinkRecord { to_url: u.into(), kind: "a", anchor: "x".into(), nofollow: false, internal: true, flags: 0 };
        s.record(&page("https://a/", 0xF0F0_F0F0_F0F0_F0F0), &[to("https://a/b"), to("https://a/c")]).unwrap();
        s.record(&page("https://a/b", 0xF0F0_F0F0_F0F0_F0F1), &[to("https://a/")]).unwrap();
        s.record(&page("https://a/c", 0x0F0F_0F0F_0F0F_0F0F), &[to("https://a/")]).unwrap();
        run(&s).unwrap();
        let t = s.table("SELECT url, near_duplicates, link_score FROM urls ORDER BY url").unwrap();
        let near: Vec<String> = t.rows.iter().map(|r| value_text(&r[1])).collect();
        assert_eq!(near, ["1", "1", "0"], "only / and /b are near duplicates");
        let home: f64 = value_text(&t.rows[0][2]).parse().unwrap();
        assert_eq!(home, 100.0, "the page everything links to scores highest");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
