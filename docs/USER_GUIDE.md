# ShoutingMonkey user guide

How to audit a website with ShoutingMonkey, the free SEO spider for Windows. For the full list of checks see [CHECKS.md](CHECKS.md).

- [Start a crawl](#start-a-crawl)
- [The window](#the-window)
- [Find and fix issues](#find-and-fix-issues)
- [Look at one URL](#look-at-one-url)
- [Compare crawls](#compare-crawls)
- [Configure the crawl](#configure-the-crawl)
- [JavaScript sites](#javascript-sites)
- [Check a list of URLs](#check-a-list-of-urls)
- [Export](#export)
- [Crawl files](#crawl-files)
- [Settings, themes and shortcuts](#settings-themes-and-shortcuts)
- [Troubleshooting](#troubleshooting)

## Start a crawl

1. Type the site's address in the box at the top: `example.com`, `www.example.com` or a full URL such as `https://example.com/blog/`.
2. Press **Start** or Ctrl+Enter.

The progress bar shows how much of what has been found so far is done, and the status bar shows URLs per second and how many requests are in progress. The phases are:

- **Reading robots.txt:** robots.txt and the XML sitemaps are read first, if the site has them.
- **Crawling:** pages are fetched and their links followed.
- **Sitemap URLs:** URLs that only the sitemap lists are crawled last.
- **Analysing:** near duplicates and link scores are worked out.
- **Completed.**

**Pause** stops new requests (those in flight finish). **Stop** ends the crawl and keeps everything found so far. Pressing **Start** while a crawl runs offers to stop it and start the new one.

Whether you type `example.com` or `www.example.com`, you get the same crawl: both are one site, and if one answers without redirecting but names the other as its canonical, the crawl moves to the main address and says so in **Issues → Crawl notes**.

## The window

![The main window](screenshots/main-window.png)

| Area | What it holds |
|---|---|
| **Navigator** (left) | Every tab, grouped into Site, On-page, Indexing and Linking. Expand a tab to see its filters and how many URLs each holds. A coloured square marks a filter that is an issue; red counts are errors. |
| **Results** (centre) | The URLs of the selected tab and filter. Pick a filter from the **Filter** list, type in **Search**, click a heading to sort, drag a heading's edge to resize, and right-click a heading to choose columns. |
| **Insights** (right) | **Issues**, **Overview** (a chart of the current tab), **Site Structure** (crawl depth and folders), **Response Times** and **Crawl Compare**. |
| **Inspector** (bottom) | Everything about the selected URL. |

Show or hide panels from the **View** menu. **View → Reset Layout** puts them back.

## Find and fix issues

Open **Insights → Issues**. Every issue found is listed, errors first, with:

- **Issue Type:** Error (broken: fix it), Warning (likely hurting the site) or Notice (worth knowing).
- **Priority:** High, Medium or Low, for how much fixing it matters.
- **URLs** and **% of Total:** how many URLs it affects.

Click a severity at the top (for example **3 Errors**) to list only those. Click an issue to show its URLs in the Results table; the bottom of the panel explains the issue and how to fix it. **Crawl notes** at the top says what robots.txt and the sitemaps contained, and anything unusual about the crawl.

The tables work like a spreadsheet: drag a column edge to resize it, double-click the edge to fit the text, right-click a heading for **Fit All Columns**, and click a heading to sort. Hover over cut-off text to read all of it.

## Look at one URL

Select a row in the Results table. The Inspector shows:

| Tab | Shows |
|---|---|
| URL Details | Every field recorded for the URL: status, indexability, title, description, headings, canonical, directives, word count, crawl depth, inlinks, link score, how it was found and more. Values can be selected and copied. |
| Inlinks | Every page linking to it, with link type, anchor text and follow. Click a URL to go to it. |
| Outlinks | Every link on the page, with the target's status code. |
| Image Details | The images on the page, with alt text, status and size. |
| Resources | Scripts, stylesheets and iframes the page loads. |
| SERP Snippet | How the page could look in Google results, with title and description pixel widths against the usual limits. |
| View Source | The page's HTML (turn on **Configuration → Advanced → Keep each page's HTML** first). |
| HTTP Headers | The response headers. |
| Duplicate Details | Pages with the same content, title or description. |

Right-click a row to copy the URL, open it in the browser, or jump to its inlinks, outlinks or SERP snippet.

## Compare crawls

Crawl the same site again later, then open **Insights → Crawl Compare**. It picks the previous crawl of the same site automatically (choose another from the list, or **Another file…**) and shows, for every issue, the previous and current URL counts and how many URLs were **Added** to or **Removed** from it. Select a row to list those URLs.

Issues keep the same id from crawl to crawl (the check and the URL), so a fixed issue never comes back as "new".

## Configure the crawl

Open **Configuration** in the menu bar.

| Page | Settings |
|---|---|
| Spider | Check resources and external links; crawl all subdomains; crawl outside the start folder; follow internal nofollow links; crawl canonicals and hreflang; crawl linked XML sitemaps |
| Limits | Maximum URLs, crawl depth, redirects to follow, page size to read |
| Speed | Parallel requests (default 5) and an optional limit on URLs per second per site |
| Rendering | JavaScript rendering, the wait after page load, and the browser to use |
| robots.txt | Respect it, ignore it, or crawl everything and report what it would block |
| Include / Exclude | Regular expressions for URLs to crawl or skip, with a tester |
| User-Agent | ShoutingMonkey, Googlebot, Bingbot, browsers or a custom string |
| Advanced | Response timeout, retries, and keeping each page's HTML |

**Save Configuration As…** and **Load Configuration…** keep settings in a file; **Clear Default Configuration** restores the defaults. Only crawl sites you own or have permission to crawl, and set a speed limit on sites you don't control.

## JavaScript sites

For sites that build their content with JavaScript (React, Vue, Angular, Next.js and so on), turn on **Configuration → Rendering**. ShoutingMonkey uses the Microsoft Edge or Google Chrome installed on the PC, headless and with a throwaway profile. Rendering is several times slower than reading the HTML.

The **JavaScript** tab then shows which pages were rendered, links that only appear after JavaScript runs (crawlers that don't render miss them), text added by JavaScript, and pages that failed to render.

## Check a list of URLs

Choose **Mode → List**, then **Upload List…** to paste or load the addresses. Only those URLs are fetched: nothing else is crawled. Useful for checking redirects after a migration or a list of URLs from another tool.

## Export

- **Export** above the Results table, or **Bulk Export → Export Current Tab**, saves what the table shows as CSV.
- **Bulk Export → Links:** all inlinks, all outlinks, all anchor text, and inlinks to redirects, 4xx, 5xx and unresponsive URLs.
- **Bulk Export → Images:** all image alt text, and images missing alt text.
- **Bulk Export → Issues:** an issues overview, or every issue as its own CSV.
- **Bulk Export → Reports:** crawl overview, all redirects, redirect chains, redirects to errors, non-indexable canonicals, insecure content, orphan URLs and a SERP summary.
- **Bulk Export → Sitemaps:** an XML sitemap of the indexable pages, and an images sitemap.
- **Bulk Export → Pages:** the saved HTML of every page.

## Crawl files

Every crawl is saved as `crawl.db` (SQLite) in its own folder in `Documents\ShoutingMonkey crawls`, named after the site and the time. Reopen one from **File → Recent Crawls** or **File → Open Crawl…**; **File → Save As…** copies it elsewhere. Crawl files can also be read by the command-line tool, or by any SQLite tool.

## Settings, themes and shortcuts

**File → Settings…** sets the theme (Light, Dark or the Windows setting), the text and interface size, and which panels are shown.

| Shortcut | Does |
|---|---|
| Ctrl+Enter | Start crawling |
| Ctrl+K | Command palette: type part of any action, tab or filter |
| Ctrl+F | Search the current view |
| Ctrl+C | Copy the selected URL |
| Ctrl+O | Open a crawl |
| Ctrl+S | Save a copy of the crawl |
| Ctrl+W | Close the crawl |
| Ctrl+Plus / Ctrl+Minus | Bigger or smaller interface |
| Ctrl+0 | Default interface size |
| Up / Down | Move through the table |
| Escape | Clear the selection |

## Troubleshooting

- **Windows SmartScreen warns about the exe.** It isn't code-signed. Click **More info → Run anyway**, or build it yourself.
- **Lots of "No Response" or timeouts.** The site may be throttling the crawler. Lower **Configuration → Speed**, or raise the timeout under **Advanced**.
- **Pages are missing.** Check **Issues → Crawl notes**, robots.txt (**Response Codes → Blocked by Robots.txt**), the include and exclude patterns, and the URL and depth limits. On JavaScript sites, turn on rendering.
- **JavaScript rendering won't start.** Install Microsoft Edge or Google Chrome, or point **Configuration → Rendering → Browser** at one.
- **Something looks wrong.** ShoutingMonkey is a hobby project and has bugs. Please [open an issue](https://github.com/sukhdevmiyatra/ShoutingMonkey/issues) with the site (if public), what you expected and what it showed. The log is in `%LOCALAPPDATA%\ShoutingMonkey\shoutingmonkey.log` (**Help → Open Log Folder**).
