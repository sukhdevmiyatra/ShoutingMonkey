# ShoutingMonkey check reference

Every technical SEO check ShoutingMonkey runs on a crawl: 107 issues (20 errors, 50 warnings, 37 notices) in 20 tabs. Each has a stable check id, used in reports, `--fail-on` and Crawl Compare.

This page is generated from the code (`shoutingmonkey-cli checks --markdown`), so it always matches what the app reports.

## Site

### Internal

Other filters: All, HTML, JavaScript, CSS, Images, PDF, Other.

### External

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Broken (4xx, 5xx, No Response) | Warning | Medium | Links to other sites that no longer work. Visitors who follow them hit an error page. | Update the links to a working address or remove them. Inlinks shows which pages carry them. | `external.broken` |

Other filters: All, HTML, JavaScript, CSS and Images.

### Response Codes

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Blocked by Robots.txt | Warning | High | URLs robots.txt does not allow the crawler to fetch, so search engines cannot read them either. | Confirm each should be blocked. If not, change the robots.txt rule that matches it. | `response_codes.blocked` |
| No Response | Error | High | Internal URLs that never answered: timeouts, refused connections or DNS failures. | Retry with a slower crawl or a longer timeout (Configuration > Speed and Advanced). Malformed links need fixing at the source. | `response_codes.no_response` |
| Redirection (3xx) | Warning | Low | Internal URLs that send visitors somewhere else. Every link to one costs an extra round trip. | Point links straight at the final URL. Inlinks shows where each redirect is linked from. | `response_codes.redirection` |
| Redirection (Meta Refresh) | Warning | Low | Pages that redirect with a meta refresh tag instead of an HTTP redirect. | Use a 301 redirect on the server, and link to the destination directly. | `response_codes.meta_refresh` |
| Redirect Chain | Warning | Medium | Redirects that lead to another redirect before reaching a page. | Point the first URL straight at the final destination. Bulk Export > Reports > Redirect Chains lists every hop. | `response_codes.redirect_chain` |
| Client Error (4xx) | Error | High | Internal URLs that answer 404 Not Found, 410 Gone or another client error. | Fix or remove the links to them, or redirect each to its closest replacement. | `response_codes.client_error` |
| Server Error (5xx) | Error | High | Internal URLs where the server failed to answer. Search engines drop pages that keep failing. | Check the server's error log for these URLs. Errors that come and go often mean an overloaded server. | `response_codes.server_error` |

Other filters: All, Success (2xx), All Errors (4xx, 5xx, No Response).

### URL

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Non ASCII Characters | Warning | Low | Addresses with characters outside plain ASCII. They are percent-encoded when shared and break in some tools. | Use plain ASCII words in new URLs and keep old ones working with redirects. | `url.non_ascii` |
| Underscores | Notice | Low | Addresses that separate words with underscores. Hyphens are the common choice and read better. | Use hyphens in new URLs; renaming old ones is rarely worth the redirects. | `url.underscores` |
| Uppercase | Notice | Low | Addresses with capitals. URLs are case-sensitive, so one page can end up with several addresses. | Use lowercase and redirect mixed-case variants. | `url.uppercase` |
| Parameters | Warning | Low | Addresses with query strings, which can multiply into many URLs for the same content. | Link to clean URLs and give parameter variants a canonical tag. | `url.parameters` |
| Over 115 Characters | Notice | Low | Long addresses are hard to read, share and remember, and are cut short in results. | Keep new URLs short and descriptive. | `url.over_115` |
| Multiple Slashes | Error | Low | Paths with doubled slashes, usually a template bug, which often duplicate another URL. | Fix the links producing them and redirect to the single-slash address. | `url.multiple_slashes` |
| Contains Space | Warning | Low | Addresses with spaces, which become %20 and break when copied. | Replace spaces with hyphens and redirect the old addresses. | `url.space` |
| Internal Search | Warning | Low | Pages of the site's own search. They are thin, near-endless and seldom worth crawling. | Keep them out of the index with noindex and avoid linking to them from crawlable pages. | `url.internal_search` |
| Tracking Parameters | Warning | Low | Internal links carrying campaign parameters. They duplicate URLs and overwrite the visit's real source. | Strip tracking parameters from internal links; keep them for links from other sites. | `url.tracking` |

Other filters: All.

### Security

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| HTTP URLs | Error | High | Internal URLs served without encryption. Browsers mark them not secure and traffic can be read in transit. | Serve everything over HTTPS, redirect HTTP to HTTPS, and update internal links. | `security.http` |
| Mixed Content | Error | High | HTTPS pages loading images, scripts, stylesheets or frames over HTTP. Browsers block or flag them. | Load every resource over HTTPS. The Resources tab lists the HTTP ones. | `security.mixed_content` |
| Form on HTTPS Posts to HTTP | Error | High | HTTPS pages with a form that sends what people type over plain HTTP. | Point every form action at an HTTPS address. | `security.insecure_form` |
| Missing HSTS Header | Warning | Low | HTTPS pages without Strict-Transport-Security, so browsers may still try plain HTTP first. | Send Strict-Transport-Security with a long max-age once the whole site works over HTTPS. | `security.hsts_missing` |
| HSTS Max-Age Under 6 Months | Warning | Low | HSTS headers with a short max-age, which protects visitors for only a short while. | Use a max-age of at least 15552000 seconds (180 days); a year is common. | `security.hsts_short` |
| Missing Content-Security-Policy | Notice | Low | Pages without a Content-Security-Policy to limit where scripts may load from. | Add a policy that allows only the origins the site needs; start in report-only mode. | `security.csp_missing` |
| CSP Allows unsafe-inline or unsafe-eval | Warning | Low | Content-Security-Policies that still allow inline scripts or eval, which removes most of their protection. | Replace 'unsafe-inline' with nonces or hashes and remove the code that needs 'unsafe-eval'. | `security.csp_unsafe` |
| Missing X-Content-Type-Options | Warning | Low | Pages without X-Content-Type-Options: nosniff, so browsers may guess content types. | Send X-Content-Type-Options: nosniff on every response. | `security.nosniff` |
| Missing X-Frame-Options | Warning | Low | Pages any site may embed in a frame, which enables clickjacking. | Send X-Frame-Options: SAMEORIGIN or a frame-ancestors rule in the Content-Security-Policy. | `security.framing` |
| Missing Referrer-Policy | Notice | Low | Pages without a Referrer-Policy, so full URLs may leak to other sites. | Send Referrer-Policy: strict-origin-when-cross-origin or stricter. | `security.referrer` |
| Missing Permissions-Policy | Notice | Low | Pages that do not restrict browser features such as camera, microphone or location for embedded content. | Send a Permissions-Policy that turns off the features the site does not use. | `security.permissions` |
| Cookie Without Secure | Warning | Low | Pages that set cookies without the Secure flag, so the cookie may travel over plain HTTP. | Add Secure to every cookie set over HTTPS. | `security.cookie_secure` |
| Cookie Without HttpOnly | Warning | Low | Pages that set cookies scripts can read, which helps attackers who get a script onto the page. | Add HttpOnly to every cookie no script needs. | `security.cookie_httponly` |
| Cookie Without SameSite | Notice | Low | Pages that set cookies without a SameSite policy. | Add SameSite=Lax (or Strict) to cookies. | `security.cookie_samesite` |
| Server Version Disclosed | Notice | Low | Responses that announce the server software and version, which helps attackers pick exploits. | Drop the version from the Server header and remove X-Powered-By. | `security.server_version` |

Other filters: All, HTTPS URLs.

### Performance

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Response Time Over 1 Second | Warning | Medium | Pages the server took more than a second to send. Slow pages lose visitors and crawl budget. | Cache the pages, speed up database queries, or use a CDN. | `performance.slow` |
| Response Time Over 3 Seconds | Error | High | Pages the server took more than three seconds to send. | Find what makes these pages slow on the server; caching usually helps most. | `performance.very_slow` |
| HTML Over 1 MB | Warning | Medium | Pages whose HTML alone is over 1 MB, which is slow to download and parse. | Remove inlined data, long lists and embedded assets from the HTML. | `performance.large_html` |

Other filters: All.

## On-page

### Page Titles

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Error | Medium | Pages without a title. The title is the headline of a search result. | Give every page a unique title that leads with its main topic. | `page_titles.missing` |
| Duplicate | Warning | Medium | Pages whose title another page also uses, so results look alike. | Write a distinct title for each page, or merge pages that really are the same. | `page_titles.duplicate` |
| Over 60 Characters | Notice | Medium | Titles beyond roughly 60 characters are usually cut off in results. | Put the important words first and trim the rest. | `page_titles.over_60` |
| Below 30 Characters | Notice | Medium | Very short titles leave out words that would describe the page. | Expand the title so it says what the page offers. | `page_titles.below_30` |
| Over 580 Pixels | Notice | Medium | Titles wider than about 580 pixels are truncated on desktop results. | Shorten the title; the SERP Preview tab shows how it will look. | `page_titles.over_580px` |
| Below 200 Pixels | Notice | Low | Titles using a small part of the space a result offers. | Use the room to describe the page more fully. | `page_titles.below_200px` |
| Same as H1 | Notice | Low | Pages whose title repeats the main heading word for word. | Consider wording that complements the heading. | `page_titles.same_as_h1` |

Other filters: All.

### Meta Description

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Notice | Low | Pages without a meta description, so search engines pick a snippet themselves. | Write a short summary that would make someone click. | `meta_description.missing` |
| Duplicate | Warning | Low | Pages whose description another page also uses. | Give each page its own description. | `meta_description.duplicate` |
| Over 155 Characters | Notice | Low | Descriptions beyond about 155 characters are usually cut off. | Keep the key message in the first 150 characters. | `meta_description.over_155` |
| Below 70 Characters | Notice | Low | Very short descriptions miss the chance to explain the page. | Expand the description into a proper summary. | `meta_description.below_70` |
| Over 990 Pixels | Notice | Low | Descriptions wider than about 990 pixels are truncated on desktop. | Shorten it; check the SERP Preview tab. | `meta_description.over_990px` |
| Below 400 Pixels | Notice | Low | Descriptions that fill little of the snippet. | Use the space to describe the page. | `meta_description.below_400px` |

Other filters: All.

### H1

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Error | Medium | Pages without an H1. The main heading tells readers and crawlers what the page is about. | Add one descriptive H1 to each page. | `h1.missing` |
| Empty | Warning | Medium | Pages whose first H1 has no text. | Give the heading text, or remove the empty element. | `h1.empty` |
| Duplicate | Notice | Low | Pages whose H1 another page also uses. | Give each page a heading about its own content. | `h1.duplicate` |
| Over 70 Characters | Notice | Low | Long main headings are hard to scan. | Keep the H1 short. | `h1.over_70` |
| Multiple | Warning | Medium | Pages with several H1s, which blurs the main topic. | Keep one H1 and use H2 to H6 for sections. | `h1.multiple` |

Other filters: All.

### H2

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Warning | Low | Pages without subheadings, which makes long content hard to scan. | Break longer content into sections with H2s. | `h2.missing` |
| Duplicate | Notice | Low | Pages whose first H2 matches another page's. | Check the headings describe each page's own content. | `h2.duplicate` |
| Over 70 Characters | Notice | Low | Long subheadings are hard to scan. | Keep subheadings short. | `h2.over_70` |
| Non-Sequential Headings | Notice | Low | Pages whose headings skip a level, such as an H2 followed by an H4, which confuses screen readers. | Step heading levels one at a time. | `h2.non_sequential` |

Other filters: All.

### Content

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Exact Duplicates | Error | Medium | Pages whose visible text is exactly the same as another page's. Search engines index one and ignore the rest. | Merge them with redirects or canonical tags, or give each its own content. The Duplicates tab lists the matches. | `content.exact_duplicates` |
| Near Duplicates | Warning | Medium | Pages whose main content (navigation, header and footer left out) is about 95% the same as another page's. | Merge pages that say the same thing, or make each one distinct. | `content.near_duplicates` |
| Low Content Pages | Notice | Medium | Pages with fewer than 200 words of visible text. | Check they give people enough; expand or merge thin pages. | `content.low_content` |
| Soft 404 Pages | Warning | High | Pages that answer 200 OK but read like an error page. Search engines treat them as errors anyway. | Return a real 404 or 410 for missing content, or redirect to something relevant. | `content.soft_404` |
| Missing Viewport | Warning | Medium | Pages without a viewport meta tag, which phones then show zoomed out like a desktop page. | Add <meta name="viewport" content="width=device-width, initial-scale=1">. | `content.no_viewport` |
| Missing Language | Warning | Low | Pages whose <html> has no lang attribute, so browsers and screen readers must guess the language. | Add lang to the html element, for example lang="en". | `content.no_lang` |

Other filters: All.

### Images

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Over 100 KB | Notice | Medium | Images over 100 KB, which slow down the pages showing them. | Compress them, serve WebP or AVIF, and size them for how they are displayed. | `images.over_100kb` |
| Missing Alt Text | Error | Low | Images shown on at least one page with no alt text, so screen readers have nothing to say. | Describe meaningful images in alt; mark decorative ones with alt="" deliberately. | `images.missing_alt` |
| Alt Text Over 100 Characters | Notice | Low | Images with very long alt text, which is tiring to listen to. | Keep alt text to a short description. | `images.alt_over_100` |
| Missing Size Attributes | Notice | Low | Images placed without width and height, so the page jumps around while they load. | Set width and height on each img so the browser can reserve the space. | `images.missing_size` |
| Broken | Error | Medium | Images that fail to load, leaving gaps on the pages that use them. | Fix the image address or upload the missing file. | `images.broken` |

Other filters: All.

### Structured Data

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Notice | Low | Pages without structured data, which can make them eligible for richer results. | Add JSON-LD that matches the content, such as Article, Product or Organization. | `structured_data.missing` |
| JSON-LD Parse Errors | Error | Medium | Pages with a JSON-LD block that is not valid JSON. Search engines ignore the whole block. | Fix the JSON syntax; a validator such as the Rich Results Test points at the error. | `structured_data.parse_errors` |

Other filters: All, Contains Structured Data, JSON-LD, Microdata, RDFa.

### Social Tags

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing Open Graph | Notice | Low | Pages without Open Graph tags, so links shared on social networks show whatever the network guesses. | Add og:title, og:description, og:image and og:url. | `social.no_open_graph` |
| Missing Twitter Card | Notice | Low | Pages without a twitter:card tag. | Add twitter:card (usually summary_large_image). | `social.no_twitter_card` |

Other filters: All.

## Indexing

### Canonicals

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Canonicalised | Warning | High | Pages whose canonical names a different URL, asking not to be indexed themselves. | Make sure that is intended, and link internally to the canonical URL. | `canonicals.canonicalised` |
| Missing | Warning | Medium | Pages without a canonical tag, leaving search engines to guess among duplicates. | Add a self-referencing canonical to indexable pages. | `canonicals.missing` |
| Non-Indexable Canonical | Error | High | Canonicals pointing at a redirect, an error or a noindex page; search engines may ignore them. | Point each canonical at an indexable page that answers 200. | `canonicals.non_indexable` |
| Canonical to Another Domain | Warning | High | Pages handing their search presence to a different host or protocol. | Point canonicals at the same site unless content really is syndicated. | `canonicals.other_domain` |
| Canonical Chain | Warning | Medium | Canonicals pointing at a page that itself names another canonical. | Point every page straight at the final canonical URL. | `canonicals.chain` |
| Canonical Is Relative | Warning | Low | Canonical tags written as relative URLs, which are resolved inconsistently. | Write the canonical as a full absolute URL. | `canonicals.relative` |

Other filters: All, Contains Canonical, Self Referencing.

### Directives

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Noindex | Warning | High | Pages asking search engines to leave them out of results. | Check each one really should stay out of search. | `directives.noindex` |
| Noindex Page Linked Internally | Notice | Low | Noindex pages that other pages still link to, spending links on pages that will not rank. | Remove the links, or make the page indexable if it should rank. | `directives.noindex_linked` |
| Nofollow | Warning | High | Pages asking crawlers to ignore every link on them. | Remove nofollow from pages whose links should be crawled. | `directives.nofollow` |

Other filters: All, Index, Follow, None, NoArchive, NoSnippet, Max-Snippet, Max-Image-Preview, Max-Video-Preview, NoImageIndex, Unavailable_After.

### Sitemaps

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| URLs Not in Sitemap | Notice | Medium | Indexable pages the XML sitemap leaves out. | List every page you want found (Bulk Export > Sitemaps > XML Sitemap can build one). | `sitemaps.not_in_sitemap` |
| Orphan URLs | Warning | Medium | Sitemap URLs that no crawled page links to. They were crawled only because the sitemap lists them. | Link to them from relevant pages, or drop them from the sitemap if retired. | `sitemaps.orphans` |
| Non-Indexable URLs in Sitemap | Error | Medium | Sitemap entries that are noindex or canonicalised elsewhere. A sitemap should list indexable URLs only. | Remove them from the sitemap or make them indexable. | `sitemaps.non_indexable` |
| Redirects in Sitemap | Warning | Medium | Sitemap entries that redirect. | List the final destination URL instead. | `sitemaps.redirects` |
| Errors in Sitemap | Error | High | Sitemap entries that answer with an error or not at all. | Remove dead URLs from the sitemap, or restore the pages. | `sitemaps.errors` |
| Missing Lastmod | Notice | Low | Sitemap entries without a lastmod date, which search engines use to decide what to recrawl. | Write each page's real last-modified date into the sitemap. | `sitemaps.lastmod_missing` |
| Lastmod in the Future | Warning | Low | Sitemap entries dated in the future, which makes the date worthless as a signal. | Write the real modified date. | `sitemaps.lastmod_future` |
| Malformed Lastmod | Warning | Low | Sitemap dates not in the W3C format search engines expect. | Write lastmod as YYYY-MM-DD or a full date and time with a time zone. | `sitemaps.lastmod_malformed` |

Other filters: All, URLs in Sitemap.

### Hreflang

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Missing | Warning | Medium | Pages without hreflang on a site that uses it elsewhere. | Add hreflang for every language version, including one for the page itself. | `hreflang.missing` |
| Invalid Language Code | Error | Medium | hreflang values that are not a valid language (and optional region) code, such as en, en-GB or x-default. | Use ISO 639-1 language codes, optionally with an ISO 3166-1 region. | `hreflang.invalid_code` |
| Missing Self Reference | Warning | Low | Pages whose hreflang set does not include the page itself. | Add an hreflang entry pointing at the page itself. | `hreflang.no_self` |
| Missing Return Links | Error | Medium | Pages naming a language version that does not name them back. hreflang must be mutual to work. | Add the missing hreflang link on each alternate page. | `hreflang.no_return` |

Other filters: All, Contains Hreflang.

### JavaScript

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Contains JavaScript Links | Warning | Low | Pages whose links only appear after JavaScript runs. Crawlers that do not render miss them. | Put important links in the HTML the server sends. | `javascript.links` |
| Contains JavaScript Content | Warning | Low | Pages where JavaScript adds a lot of visible text that the server's HTML does not contain. | Render important content on the server. | `javascript.content` |
| Rendering Failed | Warning | Medium | Pages the browser could not render in time; the raw HTML was used instead. | Raise the wait time in Configuration > Rendering, or check the page loads without errors. | `javascript.failed` |

Other filters: All, Rendered.

## Linking

### Links

| Issue | Type | Priority | What it means | How to fix | Check id |
|---|---|---|---|---|---|
| Internal Outlinks to Broken Pages | Error | High | Pages with at least one link or resource that leads to an error or no response. | Open the Outlinks tab, fix or remove the broken targets. | `links.to_broken` |
| Internal Outlinks to Redirects | Warning | Low | Pages linking to URLs that redirect, which adds a hop for every click. | Update the links to point at the final address. | `links.to_redirects` |
| Pages Without Internal Outlinks | Warning | Low | Pages that link nowhere else on the site, a dead end for visitors and crawlers. | Link to related pages, or check whether links are built with JavaScript. | `links.no_internal_outlinks` |
| Internal Nofollow Outlinks | Warning | Low | Pages with nofollow links to other pages on the same site. | Remove nofollow from internal links you want crawled. | `links.internal_nofollow` |
| Internal Outlinks With No Anchor Text | Notice | Low | Pages with internal links that have no text or image alt to describe the target. | Give links descriptive text, or alt text on linked images. | `links.no_anchor_text` |
| Non-Descriptive Anchor Text | Notice | Low | Pages with links such as "click here" or "read more", which say nothing about the target. | Use link text that names what the linked page is about. | `links.non_descriptive` |
| High External Outlinks | Warning | Low | Pages with more than 100 links to other sites. | Check they are useful and not spam, for example in comments. | `links.high_external` |
| High Internal Outlinks | Warning | Low | Pages with more than 1,000 internal links, so each counts for little. | Trim navigation and lists to what helps people. | `links.high_internal` |
| High Crawl Depth | Notice | Low | Pages more than four clicks from the start page get crawled and found less. | Link to important pages from higher up in the site. | `links.high_depth` |
| Only One Inlink | Notice | Low | Pages only one internal link points to, which makes them easy to lose. | Link to them from more related pages. | `links.one_inlink` |
| Nofollow Inlinks Only | Warning | Low | Pages that every internal link to them marks nofollow. | Add followed links to pages you want crawled. | `links.nofollow_inlinks_only` |

Other filters: All.
