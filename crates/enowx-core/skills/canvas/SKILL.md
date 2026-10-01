---
name: canvas
description: "Building a single self-contained HTML file that opens straight in a browser with no server and no build: a tool, a calculator, a visualiser, a dashboard, a game, a one-page explainer. The craft that keeps it from looking generated: one concrete subject, a deliberate palette and type pairing, both colour themes, a page complete at rest. Read before building or reworking a standalone page, and reach for canvas-interactive, canvas-data and canvas-ship as the work calls for them."
---

# A standalone page, built like a product

One HTML file that a person opens by double-clicking it. The styles and the
script live inside it; it needs no server, no bundler, no install. This is
the local counterpart of a hosted artifact, and the bar is the same: it
should look designed for its subject, not generated from a template, and it
should work the moment it loads.

A generated page announces itself: a blue-to-purple gradient hero, three
identical cards, a number nobody measured, an icon on every line, everything
centred, and a look that would fit any other subject equally well. This skill
is how to build one that belongs to its subject and holds up.

## 1. Settle the subject and the direction first

Before any markup:

1. **The subject**: one concrete thing, its audience, and the single job the
   page does. "A tip splitter for a dinner table", not "a calculator".
2. **The direction**: one idea from the subject's own world that decides the
   layout, the type and the details. A transit map reads like a transit
   diagram; a recipe reads like a recipe card; a synth patch sheet reads like
   a panel of knobs. Write it as one line and hold it on the whole page.
3. **The treatment**: how much design the job wants. A memo, a plan or an
   internal tool stays utilitarian (real hierarchy, considered spacing, a
   proper palette, no flourish). A landing page, a game or something kept and
   shared earns an editorial treatment with one real aesthetic risk. When
   unsure, a well-composed plain page always passes; an over-designed one
   does not.

A mood and a palette are not a direction. A direction is the concept you can
state in a line, taken from the subject, that someone else could follow.

Include at least one detail only this subject would have, as content: its
real units, its document conventions, its terms of art. It costs nothing even
on a plain page and it is what makes the page specific. Use real content
throughout, never lorem ipsum.

## 2. The file, and what it may load

The whole page is one `.html` file: a `<!doctype html>`, a `<head>` with the
meta, the title and all the CSS in one `<style>`, and the body with the
script in one `<script>` at the end. It opens from the filesystem (a
`file://` URL), so everything it needs is either inside it or loaded from a
public CDN over https.

- **Own code and small assets live inside the file.** The CSS and JS are
  inline. Small images and icons are inline SVG or `data:` URIs. This keeps
  the page to one portable file.
- **A real library loads from a CDN**, by one pinned `<script src>` placed
  before the inline script that uses it, when the page genuinely needs it (a
  charting package, a framework, a syntax highlighter). Pick the UMD build so
  it defines a global. Pin an exact version. Most pages need no library; a
  little vanilla JS beats pulling in a framework for a page that opens from
  disk. A library's own stylesheet is inlined, not linked, unless it too has
  a CDN URL.
- **Fonts** come from Google Fonts with a `<link>`, or are a system stack.
  Always declare a real fallback stack either way, so a blocked font does not
  silently become Times.
- **No `fetch` to a server, no API keys, no backend.** The page is complete
  on its own. If it needs to persist something for the same person on the
  same machine, that is `localStorage`, wrapped in try/catch, as a
  convenience only (see `canvas-data`).

A page meant to live in a project's repository still follows the project
first: an existing `DESIGN.md`, a tokens or theme file, a house style. Those
win over anything here.

## 3. The head and the reset

Start every file the same way, then build on it:

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Tip Splitter</title>
<style>
  *, *::before, *::after { box-sizing: border-box; }
  html { -webkit-text-size-adjust: 100%; }
  body { margin: 0; }
  img, svg { max-width: 100%; display: block; }
  :root { color-scheme: light dark; }
  /* the design's own tokens and rules follow */
</style>
</head>
<body>
  <!-- the page -->
  <script>
    // the page's behaviour
  </script>
</body>
</html>
```

- The `<title>` is the page's name: a short noun phrase, two to four words,
  specific to the subject (the browser tab and any bookmark show it). Not a
  sentence, not a category label alone.
- `viewport-fit=cover` plus the viewport meta lets the page run edge to edge
  on a phone.
- Give keyboard focus a visible style and respect
  `prefers-reduced-motion` (see `canvas-ship`).

## 4. Both colour themes, through tokens

A standalone page is opened on light and dark machines alike, so it must read
in both unless it deliberately commits to one visual world (a neon arcade
screen, a letterpress card). Define every colour once as a token on `:root`,
then let the OS setting redefine the tokens:

```css
:root {
  --bg: #faf8f5; --surface: #fff; --fg: #23201c; --muted: #6b655c;
  --line: #e7e2d9; --accent: #c1502e;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #1b1915; --surface: #262320; --fg: #ece7df; --muted: #a39b8e;
    --line: #39342d; --accent: #e87a54; color-scheme: dark;
  }
}
body { background: var(--bg); color: var(--fg); }
```

- Every token gets its first value on bare `:root`; the dark block only
  redefines tokens. No component rule uses a literal colour that reads in one
  theme only.
- When the page offers its own light/dark toggle, set `data-theme` on the
  root and add matching `:root[data-theme="dark"]` / `[data-theme="light"]`
  blocks so the toggle overrides the OS both ways (`canvas-data` has the
  toggle; keep the choice in `localStorage`).
- Keep the accent working on both backgrounds; do not simply invert.
- A single-theme design still sets the background and every colour
  explicitly, and sets `color-scheme` to match, so it looks right on either
  host.

## 5. Layout and type

The measures and the grid/flex mechanics live in `ui-layout` and
`ui-layout-grid`; read them for a page with real structure. The essentials
for a standalone page:

- One content width, centred or anchored, with a side gutter of at least
  16px at every width. Set the gutter once as side padding on `body` or one
  wrapper, its vertical padding as `padding-block`.
- Lay groups out with flex or grid and `gap`, not per-element margins.
- Pair a display face and a body face deliberately; keep running text near
  65 characters; set a type scale and keep to it; give headings
  `text-wrap: balance` and uppercase labels a little letter-spacing.
- Choose neutrals with a slight bias toward the accent's hue rather than a
  pure grey.
- Style cards by role, not everywhere: a border, fill, radius or shadow marks
  one element as separate, so applying the same to every block flattens the
  page.
- Nothing wider than the screen at 360px: `min-width: 0` on flex and grid
  children that hold text or a table, `max-width: 100%` on media, wide
  tables and code in their own `overflow-x: auto` box.

## 6. Complete at rest

The page is finished and readable the moment it loads, before any scroll or
interaction. A tool opens in a realistic working state (example rows clearly
marked as examples, or a plausibly filled form), not an empty shell; its
first view shows what it does. A section may animate in, but from a visible
resting state, never left at `opacity: 0` waiting for an observer that might
not run from a `file://` page.

## 7. The defaults to avoid

Current generated pages cluster on a few looks. With no direction given, do
not fall back on: a blue-to-purple (or any) gradient hero on white; warm
cream with a serif and a terracotta accent as a reflex; near-black with a
lone acid-green pop; Inter or Space Grotesk chosen as the "safe" face; emoji
as section markers; everything centred; one radius and one shadow on every
block; an accent bar on every rounded card; three identical feature cards;
invented statistics. When the user asks for one of these, follow them. When
nothing is asked, spend the freedom on the subject instead.

## 8. The sibling skills

- `canvas-interactive`: the page does something: state, events, forms,
  `<canvas>`, small games, visualisers. How to structure vanilla JS, when a
  library earns its place, and the input and feedback patterns.
- `canvas-data`: holding and shaping data in the page: `localStorage` done
  safely, a theme toggle, import and export without a server, rendering a
  list or a table from an array, drawing a chart to scale.
- `canvas-ship`: finishing: the accessibility and robustness pass, testing a
  `file://` page, keeping it one portable file, and the check before done.

## 9. Check it

- The page opens from a double-click with no server and nothing missing.
- It reads in both light and dark, every colour from a token.
- Nothing is wider than the screen at 360px; no sideways scroll on the body.
- It is complete and readable at rest, in a realistic state.
- It has one detail only this subject would have, and no invented numbers.
- It is not one of the generated defaults in section 7.
