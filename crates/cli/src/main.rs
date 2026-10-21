//! shoutingmonkey-cli: drives the engine from a terminal — crawl with a
//! progress line, export the crawl database to CSV, print a summary.
//! The desktop app (`ShoutingMonkey.exe`) is the GUI for the same engine.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use engine::checks::{self, Severity};
use engine::config::{CrawlConfig, RobotsMode};
use engine::crawl::CrawlControl;
use engine::store::{Store, value_text};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "shoutingmonkey-cli", about = "ShoutingMonkey: a website crawler for SEO audits", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(clap::Args)]
struct CrawlArgs {
    /// Seed URL(s).
    urls: Vec<String>,
    /// Crawl URLs from a text file (one per line, `#` comments allowed).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Directory for the crawl database.
    #[arg(long, short = 'o', default_value = "shout-out")]
    out: PathBuf,
    /// Crawl configuration TOML.
    #[arg(long, short = 'c', env = "SHOUTINGMONKEY_CONFIG")]
    config: Option<PathBuf>,
    /// Maximum number of URLs to crawl.
    #[arg(long)]
    max_urls: Option<u64>,
    /// Maximum link depth from the seeds.
    #[arg(long)]
    depth: Option<u32>,
    /// Parallel request slots.
    #[arg(long)]
    concurrency: Option<usize>,
    /// Requests per second per host (0 = no pacing).
    #[arg(long)]
    rps: Option<f32>,
    /// Request timeout in seconds.
    #[arg(long, short = 't')]
    timeout: Option<u64>,
    /// robots.txt: respect | ignore | report (crawl but flag blocks).
    #[arg(long, value_parser = parse_robots_mode)]
    robots: Option<RobotsMode>,
    /// Only crawl URLs matching this regex (repeatable).
    #[arg(long = "include")]
    includes: Vec<String>,
    /// Skip URLs matching this regex (repeatable).
    #[arg(long = "exclude")]
    excludes: Vec<String>,
    /// Render pages with JavaScript in a headless Edge/Chrome.
    #[arg(long)]
    render: bool,
    /// Keep raw HTML of each page in the database.
    #[arg(long)]
    store_html: bool,
    /// Do not read sitemaps.
    #[arg(long)]
    no_sitemaps: bool,
    /// Follow links onto any host, not just the seed site.
    #[arg(long)]
    unrestricted: bool,
    /// Write every problem found to this JSON file when the crawl ends.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Exit with status 2 if any problem at this severity or worse is found
    /// (error | warning | notice). For CI pipelines.
    #[arg(long, value_parser = parse_severity)]
    fail_on: Option<Severity>,
    /// Verbose logging (repeat for more).
    #[arg(long, short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    /// Crawl a website from one or more seed URLs.
    Crawl(CrawlArgs),
    /// Write a starter configuration file.
    InitConfig {
        #[arg(default_value = "shoutingmonkey.toml")]
        path: PathBuf,
    },
    /// Export a crawl database to CSV files.
    Export {
        /// The crawl database (crawl.db in the crawl's out directory).
        db: PathBuf,
        /// Output directory for CSVs.
        #[arg(short = 'o', default_value = "exports")]
        out: PathBuf,
        /// SQL SELECT to export instead of the standard tables.
        #[arg(long)]
        query: Option<String>,
    },
    /// Print a summary of a crawl database.
    Summary { db: PathBuf },
    /// List every check ShoutingMonkey runs.
    Checks {
        /// As a Markdown reference page (the source of docs/CHECKS.md).
        #[arg(long)]
        markdown: bool,
    },
    /// List the problems in a crawl database.
    Problems {
        db: PathBuf,
        /// Only problems at this severity or worse (error | warning | notice).
        #[arg(long, default_value = "notice", value_parser = parse_severity)]
        min: Severity,
        /// Write them as JSON to this file instead of printing a summary.
        #[arg(long)]
        json: Option<PathBuf>,
    },
}

fn parse_severity(s: &str) -> Result<Severity, String> {
    Severity::parse(s).ok_or_else(|| format!("unknown severity {s:?} (error | warning | notice)"))
}

fn parse_robots_mode(s: &str) -> Result<RobotsMode, String> {
    match s.to_ascii_lowercase().as_str() {
        "respect" => Ok(RobotsMode::Respect),
        "ignore" => Ok(RobotsMode::Ignore),
        "report" => Ok(RobotsMode::IgnoreButReport),
        other => Err(format!("unknown robots mode {other:?} (respect | ignore | report)")),
    }
}

impl CrawlArgs {
    fn apply(&self, cfg: &mut CrawlConfig) {
        if let Some(v) = self.max_urls { cfg.max_urls = v; }
        if let Some(v) = self.depth { cfg.max_depth = v; }
        if let Some(v) = self.concurrency { cfg.concurrency = v; }
        if let Some(v) = self.rps { cfg.rps_per_host = v; }
        if let Some(v) = self.timeout { cfg.timeout_secs = v; }
        if let Some(v) = self.robots { cfg.robots_mode = v; }
        if !self.includes.is_empty() { cfg.include_patterns = self.includes.clone(); }
        if !self.excludes.is_empty() { cfg.exclude_patterns = self.excludes.clone(); }
        if self.render { cfg.render_js = true; }
        if self.store_html { cfg.store_html = true; }
        if self.no_sitemaps { cfg.follow_sitemaps = false; }
        if self.unrestricted { cfg.restrict_to_seed_domain = false; }
    }

    fn seeds(&self) -> anyhow::Result<Vec<String>> {
        let mut seeds = self.urls.clone();
        if let Some(list) = &self.file {
            let text = std::fs::read_to_string(list)
                .with_context(|| format!("reading list {}", list.display()))?;
            seeds.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(String::from),
            );
        }
        anyhow::ensure!(!seeds.is_empty(), "provide a seed URL or --file list");
        Ok(seeds)
    }
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| level.into());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Problems per severity, worst first, as "3 errors, 1 warning, 0 notices".
fn tally(findings: &[checks::Finding]) -> String {
    Severity::ALL
        .iter()
        .map(|s| {
            let n = findings.iter().filter(|f| f.severity == s.label()).count();
            format!("{n} {}", if n == 1 { s.label().to_lowercase() } else { s.plural().to_lowercase() })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_report(findings: &[checks::Finding], path: &std::path::Path) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(findings)?)?;
    println!("Report:   {} ({} problems)", path.display(), findings.len());
    Ok(())
}

async fn run_crawl(args: CrawlArgs) -> anyhow::Result<std::process::ExitCode> {
    init_logging(args.verbose);
    let mut cfg = match &args.config {
        Some(p) => CrawlConfig::load(p).with_context(|| format!("loading config {}", p.display()))?,
        None => CrawlConfig::default(),
    };
    args.apply(&mut cfg);
    let seeds = args.seeds()?;
    std::fs::create_dir_all(&args.out)?;
    let db_path = args.out.join("crawl.db");

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner} {msg}").expect("static template"));
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("starting…");

    let control = CrawlControl::new();
    {
        let control = control.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                control.cancel();
            }
        });
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let started = tokio::time::Instant::now();
    let crawl = tokio::spawn(engine::crawl::run(cfg, db_path.clone(), seeds, Some(tx), control));
    while let Some(p) = rx.recv().await {
        pb.set_message(format!(
            "{:>6} crawled  {:>7} queued  {:>4} errors  {}",
            p.crawled,
            p.queued,
            p.errors,
            p.current_url.as_deref().unwrap_or(""),
        ));
    }
    let summary = crawl.await??;
    pb.finish_with_message(format!(
        "{}: {} URLs crawled ({} errors) in {:.1}s",
        if summary.cancelled { "stopped" } else { "done" },
        summary.crawled,
        summary.errors,
        started.elapsed().as_secs_f32()
    ));
    println!("\nDatabase: {}", db_path.display());
    let store = Store::open_readonly(&db_path)?;
    let findings = checks::findings(&store, Severity::Notice)?;
    println!("Problems: {}", tally(&findings));
    if let Some(path) = &args.report {
        write_report(&findings, path)?;
    }
    if let Some(level) = args.fail_on {
        let failing = findings.iter().filter(|f| Severity::parse(f.severity).is_some_and(|s| s <= level)).count();
        if failing > 0 {
            eprintln!("Failing:  {failing} problems at {} level or worse", level.label().to_lowercase());
            return Ok(std::process::ExitCode::from(2));
        }
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn run_problems(db: PathBuf, min: Severity, json: Option<PathBuf>) -> anyhow::Result<()> {
    let store = Store::open_readonly(&db)?;
    let findings = checks::findings(&store, min)?;
    if let Some(path) = json {
        return write_report(&findings, &path);
    }
    println!("{}", tally(&findings));
    let mut by_check: Vec<(&str, &str, usize)> = vec![];
    for f in &findings {
        match by_check.iter_mut().find(|(c, ..)| *c == f.problem) {
            Some(entry) => entry.2 += 1,
            None => by_check.push((&f.problem, f.severity, 1)),
        }
    }
    for (name, severity, n) in by_check {
        println!("  {severity:<8} {n:>6}  {name}");
    }
    Ok(())
}

fn run_export(db: PathBuf, out: PathBuf, query: Option<String>) -> anyhow::Result<()> {
    let store = Store::open_readonly(&db)?;
    let written = match query {
        Some(q) => {
            std::fs::create_dir_all(&out)?;
            let path = out.join("query.csv");
            engine::export::export_query(&store, &q, &path)?;
            vec![path]
        }
        None => engine::export::export_all(&store, &out)?,
    };
    for p in written {
        println!("wrote {}", p.display());
    }
    Ok(())
}

fn run_summary(db: PathBuf) -> anyhow::Result<()> {
    let store = Store::open_readonly(&db)?;
    println!("{} URLs", store.url_count()?);
    for (label, sql) in [
        ("by type", "SELECT resource_type, COUNT(*) FROM urls GROUP BY 1 ORDER BY 2 DESC"),
        ("by indexability", "SELECT indexability, COUNT(*) FROM urls GROUP BY 1 ORDER BY 2 DESC"),
        ("by status", "SELECT status, COUNT(*) FROM urls GROUP BY 1 ORDER BY 2 DESC"),
    ] {
        println!("{label}:");
        for row in store.table(sql)?.rows {
            let key = value_text(&row[0]);
            println!("  {:>22}  {}", if key.is_empty() { "-" } else { &key }, value_text(&row[1]));
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<std::process::ExitCode> {
    let ok = |r: anyhow::Result<()>| r.map(|()| std::process::ExitCode::SUCCESS);
    match Cli::parse().cmd {
        Command::Crawl(args) => run_crawl(args).await,
        Command::InitConfig { path } => {
            std::fs::write(&path, CrawlConfig::sample_toml())?;
            println!("wrote {}", path.display());
            Ok(std::process::ExitCode::SUCCESS)
        }
        Command::Export { db, out, query } => ok(run_export(db, out, query)),
        Command::Summary { db } => ok(run_summary(db)),
        Command::Problems { db, min, json } => ok(run_problems(db, min, json)),
        Command::Checks { markdown: true } => {
            print!("{}", checks::reference_markdown());
            Ok(std::process::ExitCode::SUCCESS)
        }
        Command::Checks { markdown: false } => {
            for (vi, fi, p) in checks::problems() {
                println!("{:<8} {:<40} {}", p.severity.label(), checks::VIEWS[vi].filters[fi].slug, checks::problem_name(vi, fi));
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
    }
}
