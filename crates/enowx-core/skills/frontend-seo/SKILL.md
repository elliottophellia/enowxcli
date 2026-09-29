---
name: frontend-seo
description: "Making public pages findable and shareable: HTML crawlers can read, titles and descriptions, canonical URLs, Open Graph and social cards, structured data from real facts, sitemaps and robots, internationalised pages, and speed as a signal. Read before building a public page or site."
---

# Findable and shareable pages

The generated public site renders its content in the browser, so crawlers
and link previews get an empty shell; every page has the same title; the
"not found" page answers `200`; a fake five-star rating sits in JSON-LD;
staging is indexed; and the share card is blank. Search engines and
social apps read HTML and trust honest signals. This is how to give them
both. It applies to public pages; the screens behind a sign-in need only
a title each. The whole frontend is in `frontend`.

## 1. Content in the served HTML

- Public pages are static or server-rendered (`frontend`, section 2): the
  title, description, canonical, social tags, structured data, main text
  and links are in the HTML the server sends.
- Google renders JavaScript, but later and within a budget; link previews
  (Slack, WhatsApp, LinkedIn, Facebook, X), many other search engines and
  most AI crawlers do not run it at all. A client-rendered page is indexed
  late or poorly and shares as a blank card.
- Links are `<a href="/path">`: crawlers follow those, not click handlers,
  `href="#"` or `javascript:`.
- Status codes tell the truth: `404` for a missing page (a "not found"
  page with `200` is a soft 404), `410` for removed for good, `301` or
  `308` for moved, `503` with `Retry-After` for maintenance. An SPA on a
  static host answers `200` for every path: render public pages on the
  server, or configure real 404s.
- Check what a crawler gets: `curl -s <url>` or view-source, not the
  DevTools element tree.

## 2. Titles, descriptions, headings

- `<title>` 50 to 60 characters, unique per page, the page's subject
  first and the brand last: "Invoice reminders for small agencies |
  [Brand]". Search engines rewrite titles that are vague or stuffed.
- Meta description 120 to 160 characters, specific to the page: what it
  offers and for whom. It is not a ranking factor, but it is often the
  snippet people read before they click.
- One `h1` saying what the page is about, then headings in order.
- Next.js: `metadata` or `generateMetadata`, with `metadataBase` and a
  title template in the root layout. Nuxt: `useSeoMeta`. SvelteKit:
  `<svelte:head>`. Astro: the layout's `<head>`, fed by page props. React
  19 hoists `<title>` and `<meta>` rendered anywhere into the head.

```ts
// app/layout.tsx
export const metadata: Metadata = {
  metadataBase: new URL("https://example.com"),
  title: { default: "[Brand]", template: "%s | [Brand]" },
};

// app/pricing/page.tsx
export const metadata: Metadata = {
  title: "Pricing",
  description: "[What the plans include and who each is for, 120 to 160 characters]",
  alternates: { canonical: "/pricing" },
};
```

## 3. One URL per page

- A `<link rel="canonical">` on every indexable page, absolute, pointing
  to itself in its preferred form: `https`, the chosen host (with or
  without `www`), the chosen trailing slash, no tracking parameters.
- The other forms redirect to it with `301` or `308` in one hop; internal
  links point at the final URL. Next.js `redirects()` with `permanent:
  true` sends `308`, which search engines treat like `301`.
- A moved page keeps its redirect for at least a year, ideally for good.
- Parameters that do not change the content (`utm_*`, `gclid`, `ref`)
  canonicalise to the clean URL; session ids never appear in URLs.
- Readable, stable URLs: lowercase, words joined by hyphens
  (`/guides/sending-invoices`), no file extensions, no dates or ids unless
  they mean something. Change a slug only with a redirect.

## 4. robots.txt and noindex

- `robots.txt` controls crawling, not indexing, and anyone can read it:
  it is never a security control, and listing secret paths in it
  advertises them.
- A URL blocked in `robots.txt` can still be indexed from links, without
  its content. To keep a page out of results, let it be crawled and send
  `noindex`; a page both blocked and `noindex` is never seen saying
  `noindex`.
- `noindex` (`<meta name="robots" content="noindex">`, or the
  `X-Robots-Tag: noindex` header for PDFs and whole environments) on
  staging and preview deployments (behind a password as well), internal
  search results, carts and accounts, thank-you pages, and thin filter
  combinations.
- Never block CSS or JavaScript: Google needs them to render the page.
- Whether to admit AI crawlers is the owner's call; ask. Each has a
  user agent to name: `GPTBot`, `ClaudeBot`, `CCBot`, and `Google-Extended`
  (a token for Gemini training, separate from Search).

```text
User-agent: *
Disallow: /api/
Disallow: /search

Sitemap: https://example.com/sitemap.xml
```

## 5. Sitemaps

- Only canonical, indexable URLs that answer `200`: nothing redirected,
  `noindex`ed or blocked.
- `lastmod` set to when the content really changed; search engines ignore
  it once it proves unreliable, and Google ignores `changefreq` and
  `priority` altogether.
- At most 50,000 URLs or 50 MB uncompressed per file; beyond that, a
  sitemap index.
- Generated from the routes and content, never written by hand: Next's
  `app/sitemap.ts`, `@astrojs/sitemap`, `@nuxtjs/sitemap`, a SvelteKit
  `sitemap.xml/+server.ts` endpoint. Listed in `robots.txt` and submitted
  in Google Search Console and Bing Webmaster Tools.

## 6. Open Graph and social cards

- `og:title`, `og:description`, `og:url` (the canonical), `og:type`
  (`website` or `article`), `og:site_name`, `og:image` as an absolute
  `https` URL of 1200 by 630px with `og:image:alt`, and `twitter:card` set
  to `summary_large_image`; X reads the `og:` tags for the rest.

```html
<meta property="og:title" content="[Page title, without the brand suffix]">
<meta property="og:description" content="[One specific sentence about this page]">
<meta property="og:url" content="https://example.com/features/reminders">
<meta property="og:type" content="website">
<meta property="og:image" content="https://example.com/og/reminders.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="[What the image shows]">
<meta name="twitter:card" content="summary_large_image">
```

- An image per page where it helps (articles, products, docs): Next's
  `opengraph-image.tsx` with `ImageResponse` from `next/og`, or Satori with
  resvg elsewhere. The page's title large, the brand small, text kept to
  the middle (some apps crop to a square), readable when shown small; under
  about 1 MB.
- Platforms cache cards for days: Facebook's Sharing Debugger and
  LinkedIn's Post Inspector fetch them again; a changed image gets a new
  URL.

## 7. Structured data from real facts

- JSON-LD in a `<script type="application/ld+json">`, describing only what
  is visible on the page and true. Misleading markup can earn a manual
  action and loses rich results.

| Type | For | Needs |
|---|---|---|
| `Organization` | the home page | `name`, `url`, `logo`, `sameAs` (official profiles) |
| `WebSite` | the home page | `name` (the site name shown in results), `url` |
| `BreadcrumbList` | pages under a hierarchy | each crumb's `name`, `item`, `position` |
| `Article`, `BlogPosting` | articles | `headline`, `datePublished`, `dateModified`, `author` (a `Person` with `name` and `url`), `image` |
| `Product` with `Offer` | product pages | `name`, `image`, real `price`, `priceCurrency`, `availability` |
| `LocalBusiness` (or a subtype) | a real business | `name`, `address`, `telephone`, real `openingHoursSpecification`, `geo` |
| `FAQPage` | only when the questions and answers are on the page | each `Question` and `acceptedAnswer` |

- Never: invented ratings or reviews, `aggregateRating` without real
  reviews shown on the page, review stars for your own business (Google
  ignores self-serving reviews for `LocalBusiness` and `Organization`), or
  facts the business did not give you (`writing`).
- Retired features need no markup: the sitelinks search box (2024), and
  FAQ rich results outside well-known government and health sites (2023).
- In a React tree, escape `<` so a value cannot close the script tag
  (`frontend-security`):

```tsx
<script type="application/ld+json"
  dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
```

- Validate with Google's Rich Results Test and the Schema Markup Validator
  (validator.schema.org).

## 8. Lists, filters and pagination

- Paginated lists: each page has its own URL (`?page=2`) with a canonical
  to itself, not to page 1, and plain `<a href>` links between pages.
  Google has ignored `rel="next"` and `rel="prev"` since 2019.
- Infinite scroll is backed by those paginated URLs, so crawlers can reach
  every item (`frontend-data`).
- Filters and facets: choose the few combinations worth a landing page
  (`/shoes/women`, with its own title and text), and keep the rest out
  with a canonical to the main list or `noindex`. On a large catalogue,
  disallow the filter parameters in `robots.txt` instead, and then do not
  expect their canonical or `noindex` to be read.
- Internal links with descriptive text ("pricing for agencies", never
  "click here"); every indexable page reachable within a few clicks, not
  only from search or the sitemap; breadcrumbs on deep pages.

## 9. Languages and regions

- One URL per language (`/en/pricing`, `/id/harga`, or subdomains), never
  a language chosen by cookie or `Accept-Language` at one URL: crawlers
  mostly send neither. `<html lang>` matches the page (`i18n`).
- `hreflang` alternates on every version, listing every version including
  itself, plus `x-default`; codes are ISO 639-1 languages with optional
  ISO 3166-1 regions (`en-GB`, `pt-BR`; never `en-UK`), and the links are
  reciprocal.

```html
<link rel="alternate" hreflang="en" href="https://example.com/en/pricing">
<link rel="alternate" hreflang="id" href="https://example.com/id/harga">
<link rel="alternate" hreflang="x-default" href="https://example.com/en/pricing">
```

- No automatic redirect by IP or browser language: suggest the other
  version in a banner instead.

## 10. Images, speed, mobile

- Images: `alt` text for people (`frontend-accessibility`), descriptive
  file names (`kopi-susu-250ml.avif`, not `IMG_4032.avif`), `width` and
  `height`, content images as `<img>` rather than CSS backgrounds.
- Core Web Vitals count as a page experience signal, small next to
  relevance; fast pages mostly win on the people who stay
  (`frontend-performance`).
- Mobile-first indexing: the phone version is the one indexed, so it
  carries the same content, links, metadata and structured data as the
  desktop one; `<meta name="viewport" content="width=device-width,
  initial-scale=1">`.

## Check it

- Build, serve the production output (started and stopped in one command,
  as `backend` section 7 shows), and read it as a crawler does:

```sh
curl -s http://localhost:3000/pricing | grep -iE '<title>|name="description"|rel="canonical"|og:|ld\+json'
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/no-such-page   # 404
curl -sI http://localhost:3000/old-pricing | grep -iE "^(HTTP|location)"      # 301 or 308, one hop
```

- Titles and descriptions unique across the sitemap (a crawl with
  Screaming Frog, free up to 500 URLs, or a script over `sitemap.xml`).
- Rich Results Test for each template with structured data; the sharing
  debuggers for a card; Search Console's URL Inspection (live test) for
  the rendered HTML and indexing status.
- Staging answers with `noindex`; production does not.
- `preview`: one `h1`, images with alt text, no dead links.

## Avoid

Public content rendered only in the browser; one title for every page; a
"not found" page answering `200`; canonicals to page 1 from every page;
`noindex` on a page also blocked in `robots.txt`; secrets listed in
`robots.txt`; staging left indexable; invented ratings, reviews or facts
in structured data; FAQ markup for questions not on the page; keyword
stuffing, hidden text, doorway pages, or content shown only to crawlers;
automatic redirects by language; a sitemap full of redirects.
