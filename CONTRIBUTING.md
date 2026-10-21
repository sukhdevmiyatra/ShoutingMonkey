# Contributing to ShoutingMonkey

Thanks for helping. Bug reports, ideas and pull requests are all welcome.

## Reporting a bug

Open an issue with:

- the site you crawled (if it is public) and the settings you changed,
- what you expected and what ShoutingMonkey showed,
- the relevant lines of `%LOCALAPPDATA%\ShoutingMonkey\shoutingmonkey.log` (**Help → Open Log Folder**).

## Working on the code

```bat
cargo test --workspace
cargo clippy --workspace --all-targets
```

- **A new check** is one entry in `crates/engine/src/checks.rs`: a stable slug, a label, a SQL condition, a severity, a priority, what it means and how to fix it. Add a page to the local test site in `crates/engine/tests/broken_site.rs` that triggers it (the test fails if any check never fires), then regenerate the reference: `cargo run -p cli -- checks --markdown > docs/CHECKS.md`.
- **Slugs are permanent.** Crawl Compare and CI reports key on them. If one must change, add the old name to `RENAMED` in `checks.rs`.
- **UI changes:** the desktop UI tests render the real window offscreen (`cargo test -p gui -- --ignored --test-threads=1`, with `SHOUTINGMONKEY_HOME` set to a scratch folder). `readme_screenshots` refreshes the pictures in `docs/screenshots`.
- Keep the code style of the file you are in, and keep the app offline: no telemetry, no network calls other than the crawl.

## Licence

By contributing you agree that your contribution is released under the [MIT licence](LICENSE). Write your own code: do not copy code, text or images from other SEO tools.
