---
name: frontend-performance
description: "Making a web frontend fast: Core Web Vitals and budgets, what loads first, images and fonts, JavaScript weight and code splitting, rendering strategy and hydration, INP and long tasks, layout shift, caching and a CDN, long lists, and measuring in the lab and the field. Read before building a page that must load fast, and when one is slow."
---

# A fast frontend

The generated page is fast on the laptop that built it and slow
everywhere else: a 3000px hero image lazy-loaded, four font weights from a
third-party CDN, 900 kB of JavaScript for a page of text, a chart library
in the first bundle, 5,000 rows rendered at once, a banner that pushes the
page down as it arrives, and nothing measured. This is how to budget,
build and measure for a mid-range phone on a mobile network, which is
where most visits come from. Animation cost is `motion-performance`; the
whole frontend is `frontend`.

## 1. Budgets

Core Web Vitals, at the 75th percentile of real visits, on mobile:

| Metric | Good | Poor | Measures |
|---|---|---|---|
| LCP, Largest Contentful Paint | 2.5s or less | over 4s | when the main content appears |
| INP, Interaction to Next Paint | 200ms or less | over 500ms | how fast taps, clicks and keys get a response |
| CLS, Cumulative Layout Shift | 0.1 or less | over 0.25 | how much the page jumps |

- Behind them: time to first byte 0.8s or less, first contentful paint
  1.8s or less.
- JavaScript loaded before the page is usable, compressed: a content page
  under about 150 kB, an application shell under about 250 kB. Each route
  then adds only its own code.
- The LCP image usually fits in 100 to 200 kB as AVIF or WebP at its shown
  size; fonts in two to four files of 15 to 40 kB each once subset.
- Written down and checked in CI (section 10), or the budget is gone by the
  third feature.

## 2. What loads first

- A fast first byte: HTML from a CDN (static or cached), rendered near the
  data when it is dynamic, and no redirect chain (`http` to `https` to
  `www` to `/en` is three round trips).
- Find the LCP element (the Performance panel, or the web-vitals
  attribution build) and make it early. An image: an `<img>` in the HTML
  (not a CSS background, not inserted by a script), `fetchpriority="high"`,
  never `loading="lazy"`, preloaded only when discovered late. Text: its
  font must not hold it back (section 4).
- CSS in the `head`, small; scripts `defer` or `type="module"`; no
  synchronous third-party script in the `head`.
- `preconnect` to the one to three other origins needed at once (an image
  CDN, the API); more costs more than it saves. Fonts need `crossorigin`
  on their preconnect and preload; images do not.

```html
<link rel="preconnect" href="https://img.example-cdn.net">
<link rel="preload" href="/fonts/body-var.woff2" as="font" type="font/woff2" crossorigin>
<img src="/img/hero-1200.avif"
     srcset="/img/hero-800.avif 800w, /img/hero-1200.avif 1200w, /img/hero-2000.avif 2000w"
     sizes="(min-width: 1024px) 50vw, 100vw"
     width="1200" height="800" fetchpriority="high" alt="[What the photo shows]">
```

- Pages people are likely to open next can load before the click: the
  framework's link prefetching, or Speculation Rules
  (`<script type="speculationrules">`) in Chromium browsers.
- Keep pages eligible for the back/forward cache, which makes Back
  instant: no `unload` listeners, no `Cache-Control: no-store` on HTML
  without a reason. DevTools, Application, Back/forward cache tests it.

## 3. Images

- AVIF, with WebP as the fallback (`<picture>`, or an image CDN choosing by
  the `Accept` header); SVG for logos and icons; PNG only where lossless
  matters and WebP will not do.
- `srcset` with widths and a `sizes` matching the layout, so a phone never
  downloads the desktop file; at most twice the displayed width.
- `width` and `height` on every `img` (or `aspect-ratio`), so its space is
  kept before it loads.
- `loading="lazy"` and `decoding="async"` below the fold, never on the LCP
  image.
- Let the framework do it: `next/image` (with `sizes` whenever it uses
  `fill`), Astro's `<Image>` and `<Picture>`, Nuxt Image, `@unpic` for image
  CDNs, `vite-imagetools` in Vite. Quality around 75 is a good start.
- Video: a poster, `preload="none"` or `"metadata"`, no autoplaying hero
  video on phones; decorative loops `muted playsinline`, short and small.

## 4. Fonts

- `woff2` only, self-hosted (next/font, Fontsource packages, or files in
  the repository), subset to the scripts the pages use (`unicode-range`,
  `pyftsubset`); one variable font rather than four static weights; two
  families at most (`ui`).
- Preload only the one or two files used above the fold.
- `font-display: swap` shows text at once in the fallback; `optional` uses
  the web font only if it arrives within about 100ms, and never shifts.
- Match the fallback's metrics so the swap does not move text. next/font
  does it by itself; elsewhere Fontaine or Capsize compute the overrides
  for your font:

```css
@font-face {
  font-family: "Body Fallback";
  src: local("Arial");
  size-adjust: 104%;       /* computed for the real font, never copied */
  ascent-override: 92%;
  descent-override: 24%;
  line-gap-override: 0%;
}
body { font-family: "Body", "Body Fallback", sans-serif; }
```

## 5. JavaScript weight

- Split by route: automatic in Next, SvelteKit, Nuxt, Astro and TanStack
  Router; `lazy` in React Router; `React.lazy` elsewhere.
- Load heavy widgets when needed, with `import()`: rich text and code
  editors, charts, maps, PDF viewers, emoji pickers, syntax highlighters;
  browser-only ones with `next/dynamic` and `ssr: false`, in a client component.
- Imports that tree-shake: named imports from ES modules, icons one by
  one, `date-fns` functions rather than a whole date library, no
  `import * as`. `Intl` formats dates and numbers at no bundle cost.
- Find the weight: `npx vite-bundle-visualizer`, `rollup-plugin-visualizer`,
  `@next/bundle-analyzer`, or `npx source-map-explorer dist/assets/*.js`.
  Remove unused packages (`npx knip`) and duplicate versions
  (`pnpm dedupe`).
- Compile for the browsers you support (`build.target`, `browserslist`);
  no polyfills for what they all have.
- Third-party scripts are the usual culprit: each tag manager, chat
  widget, A/B tool and pixel runs on the main thread. Keep few, load them
  after the page is interactive or on first use (`next/script` with
  `strategy="lazyOnload"`), and put a facade in front of heavy embeds (a
  thumbnail that loads the video player on click, a static map image).

## 6. Rendering and hydration

- Content pages: static or incrementally regenerated HTML from a CDN.
  Server-rendered pages: stream the shell at once, the slow parts behind
  `Suspense`.
- Hydration runs the page's components again in the browser, costing in
  proportion to the JavaScript and data shipped. Ship less: React Server
  Components keep markdown parsing, syntax highlighting and formatting on
  the server; Astro islands hydrate only the interactive parts
  (`client:visible`, `client:idle`).
- Pass client components only the fields they use: a whole record
  serialised into the page for one name is sent twice, as markup and as
  data.
- Public content rendered only in the browser has the slowest LCP of all:
  nothing shows until the bundle has loaded, run and fetched.
- Long pages: `content-visibility: auto` with
  `contain-intrinsic-size: auto 600px` on sections below the fold skips
  their rendering until they come near.

## 7. INP: answer within a frame or two

- A long task is 50ms or more of main-thread work. An interaction waits
  for the task ahead of it, runs its handlers, then waits for the next
  paint; INP is close to the slowest of these on the page.
- Handlers do the visible part first (the button pressed, the item
  checked) and defer the rest (analytics, recalculation) until after the
  paint. Long work runs in pieces, yielding between them:

```ts
/** Lets the browser paint and handle input before the next piece of work. */
function yieldToMain(): Promise<void> {
  const scheduler = (globalThis as { scheduler?: { yield?: () => Promise<void> } }).scheduler;
  return scheduler?.yield ? scheduler.yield() : new Promise((resolve) => setTimeout(resolve, 0));
}

let deadline = performance.now() + 40;
for (const row of rows) {
  handle(row);
  if (performance.now() > deadline) {
    await yieldToMain();
    deadline = performance.now() + 40;
  }
}
```

- React: `useTransition` for an update that renders a lot (a heavy tab, a
  large list filtered), `useDeferredValue` for a slow view that follows
  fast input, so typing stays instant.
- Debounce work triggered by typing (200 to 300ms); observers
  (`IntersectionObserver`, `ResizeObserver`) instead of scroll and resize
  handlers.
- Virtualise lists past about 200 rows, sooner when rows are heavy:
  TanStack Virtual (`useVirtualizer({ count, getScrollElement,
  estimateSize: () => 44, overscan: 8 })`) or react-virtuoso. Rows outside
  the window are not in the DOM, so find-in-page misses them: give the
  list its own search.
- Keep the DOM small: thousands of nodes make every style and layout pass
  slower. Read sizes (`getBoundingClientRect`, `offsetHeight`) together,
  then write styles together, never alternating in a loop.
- Heavy computation (parsing a large file, image work, diffing) goes to a
  Web Worker; Comlink makes it a function call.

## 8. CLS: nothing jumps

- Reserve space for everything that loads: images and video (`width` and
  `height`), embeds and ads (`aspect-ratio` or `min-height` on the slot),
  late banners (overlaid, or their space kept from the start).
- Never insert content above what the reader is looking at: new items
  arrive below, or behind a "Show 3 new" button.
- Fallback font metrics (section 4); skeletons the size of the content.
- Animate `transform` and `opacity`, never `top`, `height` or `margin`
  (`motion-performance`).
- Shifts within 500ms after a tap or key press do not count: an accordion
  opening on click is fine.

## 9. Caching and delivery

- Built assets with hashed names, cached for a year:
  `Cache-Control: public, max-age=31536000, immutable`.
- HTML revalidated on each visit (`no-cache` with an `ETag`), or cached at
  the CDN briefly with `stale-while-revalidate`. Long-cached HTML points at
  files a later deploy removed (`frontend-errors`).
- A CDN in front, Brotli for text (pre-compressed at build), HTTP/2 or 3.
- A service worker only when offline use or installing matters (Workbox,
  `vite-plugin-pwa`, Serwist): it is a cache that serves old code when its
  update flow is wrong.

```nginx
# An SPA on nginx
location /assets/ { add_header Cache-Control "public, max-age=31536000, immutable"; }
location / {
  add_header Cache-Control "no-cache";
  try_files $uri /index.html;
}
```

## 10. Measuring

- Lab, while building: Lighthouse in DevTools or `npx lighthouse <url>
  --view` (mobile and throttled by default); the Performance panel with
  the CPU slowed 4x to 6x and the network at Slow 4G, whose live metrics
  show LCP, INP and CLS as you use the page, with long tasks and layout
  shifts on their tracks.
- Field, what users get: PageSpeed Insights and the Search Console Core
  Web Vitals report (Chrome UX Report data, 28 days, p75), and your own
  numbers from the `web-vitals` library:

```ts
import { onCLS, onINP, onLCP } from "web-vitals/attribution";
import type { Metric } from "web-vitals";

const send = (metric: Metric) => navigator.sendBeacon("/api/vitals", JSON.stringify(metric));
onCLS(send);
onINP(send);
onLCP(send);
```

- In CI: `size-limit` (a `.size-limit.json` like this) fails the build over
  budget; Lighthouse CI (`lhci autorun`) asserts on the key pages.

```json
[{ "path": "dist/assets/index-*.js", "limit": "250 kB", "gzip": true }]
```

- Now and then, a real mid-range Android phone: the laptop is not the
  audience.

## Check it

- `pnpm build`: read each chunk's size; open the bundle analyser for
  anything unexpected.
- Lighthouse (mobile) on the production build of the key pages: LCP, CLS
  and total blocking time within budget, and the LCP element is the one
  you meant.
- The Performance panel, throttled: tap the main controls, no task over
  200ms; scroll the long lists, no stutter.
- `preview` with `motion: true` reports long frames and layout shift.

## Avoid

A lazy-loaded or CSS-background LCP image; images with no size; the
desktop file sent to phones; fonts from a third-party CDN, four static
weights, no fallback metrics; a chart, editor or map in the first bundle;
`import *` from big libraries; tag managers loaded first; public content
rendered only in the browser; thousands of rows in the DOM; content
inserted above the reader; long-cached HTML; budgets nobody checks.
