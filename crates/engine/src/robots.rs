#[derive(Debug, Clone, Default)]
pub struct AgentGroup {
    pub agents: Vec<String>,
    /// Raw rule strings as written.
    pub allow: Vec<String>,
    pub disallow: Vec<String>,
    /// Compiled rules, parallel to `allow` / `disallow`.
    pub allow_rules: Vec<Rule>,
    pub disallow_rules: Vec<Rule>,
    pub crawl_delay: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Robots {
    pub groups: Vec<AgentGroup>,
    pub sitemaps: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Rule {
    /// Plain literal prefix.
    Prefix(String),
    /// Wildcard (`*`) and/or end-anchor (`$`) pattern compiled to regex.
    /// Keeps the raw rule length, which is its specificity.
    Pattern(regex::Regex, usize),
}

fn compile_rule(raw: &str) -> Rule {
    if raw.contains('*') || raw.ends_with('$') {
        // Build regex: escape regex specials except '*', translate '*' -> '.*';
        // a trailing '$' anchors (and is consumed).
        let mut pat = String::from("^");
        let body = raw.strip_suffix('$').unwrap_or(raw);
        let anchored = body.len() != raw.len();
        for c in body.chars() {
            match c {
                '*' => pat.push_str(".*"),
                c if "()[].+^|{}?\\".contains(c) => {
                    pat.push('\\');
                    pat.push(c);
                }
                _ => pat.push(c),
            }
        }
        if anchored {
            pat.push('$');
        }
        if let Ok(re) = regex::Regex::new(&pat) {
            return Rule::Pattern(re, raw.len());
        }
    }
    Rule::Prefix(raw.to_string())
}

/// Specificity of `rule` if it matches `path_and_query`, else None.
fn match_len(rule: &Rule, path_and_query: &str) -> Option<usize> {
    match rule {
        Rule::Prefix(p) => path_and_query.starts_with(p.as_str()).then_some(p.len()),
        Rule::Pattern(re, len) => re.is_match(path_and_query).then_some(*len),
    }
}

/// Parse robots.txt text. Case-insensitive directives; comments stripped;
/// a `User-agent:` line starts a new group unless the current group has
/// no rules yet (consecutive agent lines merge into one group, per RFC 9309).
pub fn parse(text: &str) -> Robots {
    let mut groups: Vec<AgentGroup> = Vec::new();
    let mut sitemaps: Vec<String> = Vec::new();
    // Whether the last group has seen any rule line (incl. empty Disallow).
    let mut last_has_rules = false;

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        match key.as_str() {
            "sitemap" => {
                if !value.is_empty() {
                    sitemaps.push(value);
                }
            }
            "user-agent" => {
                let agent = value.to_ascii_lowercase();
                match groups.last_mut() {
                    Some(last) if !last_has_rules => last.agents.push(agent),
                    _ => {
                        groups.push(AgentGroup {
                            agents: vec![agent],
                            ..Default::default()
                        });
                        last_has_rules = false;
                    }
                }
            }
            "disallow" | "allow" | "crawl-delay" => {
                let Some(g) = groups.last_mut() else {
                    continue; // rule outside any group: ignore
                };
                last_has_rules = true;
                match key.as_str() {
                    // An empty Disallow matches nothing (RFC 9309), so it
                    // is simply not recorded.
                    "disallow" if !value.is_empty() => {
                        g.disallow_rules.push(compile_rule(&value));
                        g.disallow.push(value);
                    }
                    "allow" if !value.is_empty() => {
                        g.allow_rules.push(compile_rule(&value));
                        g.allow.push(value);
                    }
                    "crawl-delay" => g.crawl_delay = value.parse().ok(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Robots { groups, sitemaps }
}

impl Robots {
    /// Pick the group for `user_agent` (a full UA string or a bare token).
    /// A group's agent token matches when the UA contains it
    /// (case-insensitive); the longest matching token wins and `*` is the
    /// fallback. No matching group = unrestricted.
    pub fn group_for(&self, user_agent: &str) -> Option<&AgentGroup> {
        let ua = user_agent.to_ascii_lowercase();
        let mut best: Option<(&AgentGroup, usize)> = None;
        let mut wildcard: Option<&AgentGroup> = None;
        for g in &self.groups {
            for a in &g.agents {
                if a == "*" {
                    wildcard.get_or_insert(g);
                } else if !a.is_empty()
                    && ua.contains(a.as_str())
                    && best.is_none_or(|(_, len)| a.len() > len)
                {
                    best = Some((g, a.len()));
                }
            }
        }
        best.map(|(g, _)| g).or(wildcard)
    }

    /// Whether `path_and_query` (e.g. `/dir/x?q=1`) is allowed by the group.
    /// The most specific (longest) matching rule wins, Allow wins ties, and
    /// no matching rule means allowed.
    pub fn path_allowed(group: Option<&AgentGroup>, path_and_query: &str) -> bool {
        let Some(g) = group else {
            return true;
        };
        let longest = |rules: &[Rule]| {
            rules
                .iter()
                .filter_map(|r| match_len(r, path_and_query))
                .max()
        };
        match (longest(&g.allow_rules), longest(&g.disallow_rules)) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(d)) => a >= d,
        }
    }

    /// Sitemap URLs declared by this robots.txt.
    pub fn sitemap_urls(&self) -> &[String] {
        &self.sitemaps
    }

    /// What an unreachable robots.txt (5xx / network failure) means per
    /// RFC 9309: complete disallow.
    pub fn disallow_all() -> Self {
        parse("User-agent: *\nDisallow: /\n")
    }
}

/// Convenience: evaluate a parsed robots for this user agent's group.
pub fn allows_for(robots: Option<&Robots>, user_agent: &str, path_and_query: &str) -> bool {
    let g = robots.and_then(|r| r.group_for(user_agent));
    Robots::path_allowed(g, path_and_query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prefix_rules() {
        let r = parse("User-agent: *\nDisallow: /admin\nDisallow: /secret\nAllow: /admin/public\n");
        assert!(!allows_for(Some(&r), "Googlebot", "/admin/panel"));
        assert!(allows_for(Some(&r), "Googlebot", "/admin/public/more"));
        assert!(allows_for(Some(&r), "Googlebot", "/home"));
        assert!(allows_for(Some(&r), "Googlebot", "/"));
    }

    #[test]
    fn wildcard_and_anchor() {
        let r = parse("User-agent: *\nDisallow: /*.pdf$\n");
        assert!(!allows_for(Some(&r), "x", "/files/report.pdf"));
        assert!(allows_for(Some(&r), "x", "/files/report.pdf.gz"));
        assert!(allows_for(Some(&r), "x", "/files/"));
        let r2 = parse("User-agent: *\nDisallow: /*private*\n");
        assert!(!allows_for(Some(&r2), "x", "/a/private/b"));
    }

    #[test]
    fn empty_disallow_matches_nothing() {
        let r = parse("User-agent: somebot\nDisallow:\n");
        assert!(allows_for(Some(&r), "somebot", "/x"));
        // it does not cancel the group's other rules
        let r = parse("User-agent: somebot\nDisallow:\nDisallow: /x\n");
        assert!(!allows_for(Some(&r), "somebot", "/x"));
    }

    #[test]
    fn longest_rule_wins_and_allow_wins_ties() {
        let r = parse("User-agent: *\nAllow: /p\nDisallow: /p\nDisallow: /a/*.php\nAllow: /a/\n");
        assert!(allows_for(Some(&r), "x", "/page"));
        assert!(!allows_for(Some(&r), "x", "/a/index.php"));
        assert!(allows_for(Some(&r), "x", "/a/index.html"));
    }

    #[test]
    fn agent_selection_prefers_longest() {
        let r = parse("User-agent: *\nDisallow: /\n\nUser-agent: goodbot\nDisallow: /none\n");
        assert!(!allows_for(Some(&r), "otherbot", "/a"));
        assert!(allows_for(Some(&r), "goodbot", "/a"));
    }

    #[test]
    fn full_user_agent_string_matches_token() {
        let r = parse("User-agent: *\nDisallow: /\n\nUser-agent: shoutingmonkey\nDisallow: /private\n");
        let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/126.0 shoutingmonkey/0.1";
        assert!(allows_for(Some(&r), ua, "/public"));
        assert!(!allows_for(Some(&r), ua, "/private"));
    }

    #[test]
    fn merged_agent_lines_share_rules() {
        let r = parse("User-agent: a\nUser-agent: b\nDisallow: /x\n");
        assert_eq!(r.groups.len(), 1);
        assert!(!allows_for(Some(&r), "b", "/x"));
    }

    #[test]
    fn sitemaps_collected() {
        let r = parse("Sitemap: https://example.com/sitemap.xml\nUser-agent: *\nDisallow:\n");
        assert_eq!(r.sitemaps, vec!["https://example.com/sitemap.xml"]);
    }

    #[test]
    fn crawl_delay() {
        let r = parse("User-agent: *\nCrawl-delay: 2.5\nDisallow:\n");
        let g = r.group_for("any").unwrap();
        assert_eq!(g.crawl_delay, Some(2.5));
    }
}
