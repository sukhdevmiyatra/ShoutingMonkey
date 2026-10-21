# Changelog

## 0.1.0 (first public release)

- Desktop app for Windows (`ShoutingMonkey.exe`) and command-line tool (`shoutingmonkey-cli.exe`) sharing one crawler.
- 107 technical SEO checks in 20 tabs, each with a stable id, what it means and how to fix it ([docs/CHECKS.md](docs/CHECKS.md)).
- Follows links, redirects, canonicals, hreflang and meta refresh; reads robots.txt and XML sitemaps; finds orphan URLs.
- `www` and bare hosts are one site, and the main address is picked automatically.
- Optional JavaScript rendering with the installed Edge or Chrome.
- Near-duplicate detection, link score, crawl depth and folder structure, response times.
- Crawl Compare, bulk exports, XML and image sitemap generation, list mode.
- CI mode: `--report` JSON and `--fail-on` exit codes.
