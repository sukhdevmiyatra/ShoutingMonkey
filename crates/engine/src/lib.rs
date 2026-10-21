//! ShoutingMonkey engine: an asynchronous website crawler for SEO audits.
//! A breadth-first frontier feeds a pool of workers that obey robots.txt,
//! read sitemaps, classify and parse each response, optionally render it in
//! a browser, and store everything in SQLite, where the check catalogue
//! (`checks`) turns it into a list of problems.

pub mod classify;
pub mod config;
pub mod analysis;
pub mod checks;
pub mod crawl;
pub mod export;
pub mod fetch;
pub mod frontier;
pub mod norm;
pub mod parse;
pub mod render;
pub mod robots;
pub mod sitemap;
pub mod store;