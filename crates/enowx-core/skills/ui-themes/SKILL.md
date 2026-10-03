---
name: ui-themes
description: "Light and dark themes, and colour that holds in both: tokens with a value per theme, and what each kind of content needs to stay visible (icons, SVG drawings and line art, logos, photos, screenshots, charts, code, maps, borders, shadows, focus, form controls), switching without a flash, and checking both. Read before building a second theme or a theme toggle, and before any drawing, logo or chart on a page that has two themes."
---

# Light and dark themes

A theme is more than a background colour. Generated dark modes flip the page
and forget what sits on it: a line drawing in black vanishes, a logo with
black lettering on a transparent file disappears, a chart keeps its light
colours, card shadows stop showing, the focus ring blends in. Everything
drawn on the page is decided for each theme.

## 1. One set of tokens, a value per theme

- Tokens are named by role, never by colour: `--bg`, `--surface`,
  `--surface-2`, `--text`, `--text-muted`, `--border`, `--border-strong`,
  `--accent`, `--on-accent` (text on the accent), `--ink` (the colour
  drawings are made of), `--success`, `--warning`, `--danger`, `--focus`.
  Not `--black`, `--gray-900`.
- Light values on `:root`; dark values under the project's switch:
  `[data-theme="dark"]` or `.dark` (Tailwind's `dark` variant), with
  `@media (prefers-color-scheme: dark)` for a first visit where nothing is
  saved yet.
- Components use tokens only: no raw hex in a component, no per-element
  `dark:` overrides where a token would do. In Tailwind, map the tokens into
  the theme (`bg-background`, `text-foreground`) and let the variables
  switch.
- Each value is chosen, not inverted. Dark surfaces step up 2 to 4% in
  lightness per level (the `ui` skill, neutrals). An accent keeps its hue
  but usually needs a lighter step on dark (for example 38% lightness on
  light, 62% on dark) to stay at 3:1 as a control and 4.5:1 as text; the
  status colours likewise.
- `color-scheme: light` and `color-scheme: dark` on the root of each theme,
  so scrollbars, date pickers and native form controls follow; a
  `<meta name="theme-color">` per theme.

## 2. Contrast, for everything that carries meaning

- Text 4.5:1, large text 3:1.
- Icons, the lines and shapes of a drawing that shows something, chart
  marks, borders that identify a control, and focus rings: 3:1 against what
  is right behind them. Decoration that carries nothing may be faint; the
  thing the section is about may not.

## 3. Each kind of content, in both themes

- **Icons**: `currentColor`, inheriting a text token. Never a fixed
  `fill="#000"` or `stroke="black"`.
- **SVG drawings and line art** (a technical drawing, a diagram, a mascot):
  inline the SVG and draw it with tokens: `stroke="currentColor"` on its
  strokes and `color: var(--ink)` on its wrapper; fills that are surfaces
  of the drawing from `--surface-2`; the one highlighted part (a flag, a
  marker) from `--accent`. An SVG in an `<img>` cannot read the page's
  colours: inline it; or, for single-colour art, use it as a mask
  (`mask: url(ship.svg) center / contain no-repeat;
  background: var(--ink)`); or ship one file per theme and switch them with
  the theme class.
- **Logos**: a version per theme, dark lettering for light and light
  lettering for dark, switched with the theme, or an inline SVG in
  `currentColor`. Other companies' logos the same, or on a plate that stays
  light.
- **Photos**: as they are; very bright ones may dim slightly on dark
  (`filter: brightness(.9)`). Never invert a photo.
- **Screenshots of the product**: the screenshot in the matching theme when
  the product has both; otherwise framed (a border and a surface around it),
  so a white screenshot does not sit on a dark page like a hole.
- **Charts**: series colours from tokens, the same hues lighter on dark;
  gridlines at border strength; labels in the muted text colour; tooltips
  on the surface. Re-render or restyle when the theme changes
  (`ui-part-charts`).
- **Code**: a syntax theme per mode (a dual theme such as Shiki's
  `github-light` and `github-dark`), not a dark block on a light page by
  habit.
- **Maps and embeds**: the provider's dark style in the dark theme; embeds
  framed.
- **Elevation**: on dark a shadow barely shows. Raise a surface by making it
  lighter and giving it a hairline border; keep soft shadows for light.
- **Borders and dividers**: 8 to 14% above the background on dark, 8 to 12%
  below it on light; a border that marks a control reaches 3:1.
- **States**: hover and selected as surface steps; disabled still legible
  and still clearly disabled; a focus ring (2px in `--focus`, with an
  offset) visible in both.
- **Selection and the caret**: `::selection` and `caret-color` from tokens.

## 4. A worked case

A developer's portfolio with a light and a dark theme. The hero is a
technical line drawing of a ship, and a small dial of the same set sits
above a section label. Both files are drawn in `currentColor`, which looks
right, and both are loaded with `<img src="/media/antares.svg">`. An image
never takes the page's colours: inside it, `currentColor` is black. On the
light theme the drawings read; on the dark page (about `#08161d`) they are
black on near-black, 1.1:1, and the drawing that is the focal point of the
page disappears. The `preview` tool reports it in the dark pass: `img
antares.svg 540x180 drawn in #000000 on #08161d, 1.14:1, needs 3:1`.

Three fixes, each keeping one drawing for both themes:

1. Inline it as a component, so it takes the page's colour: in Vite with
   `vite-plugin-svgr` (`import Ship from "./antares.svg?react"`), in Next.js
   with `@svgr/webpack`, or pasted into a component.
2. Keep the file and reference it with `<use>`, which does inherit the
   colour: give the drawing's root group an id and write
   `<svg viewBox="0 0 720 240" role="img" aria-label="..."><use href="/media/antares.svg#drawing" /></svg>`.
3. For a single-colour drawing, use the file as a mask over a token:
   `mask: url(/media/antares.svg) center / contain no-repeat;
   background: var(--ink)`.

Then colour it for each theme:

```css
:root { --ink: #1a1d21; --accent: #c2410c; }
[data-theme="dark"] { --ink: #c9ced6; --accent: #f97316; }
.drawing { color: var(--ink); }
.drawing .flag { fill: var(--accent); }
```

`--ink` is near-black on light and a light grey on dark (above 7:1 on the
dark page); the thin lines of the drawing (the waterline, dimension lines)
use `--border-strong`, which still reaches 3:1; the one highlighted part
(the flag) keeps the accent.

## 5. Switching

- The default follows the system (`prefers-color-scheme`) unless the concept
  sets one theme, in which case there is no toggle.
- A toggle is an icon button in the header's right group: the icon set's
  sun or moon and nothing else. No visible text: not "Dark mode", not
  "Light", not a label beside or under the icon, not a switch with words on
  it. Its name is for assistive technology and the pointer, not the eye:
  an `aria-label` that says what it does ("Switch to dark theme"), the same
  in a `title`, updated when the theme changes. It is the size of the other
  icon buttons (a 40px target at least) and shows focus like them. The
  choice is saved, and applied before the first paint by a small script in
  `<head>` that sets the class or attribute, so the wrong theme never
  flashes (`next-themes` does this in Next.js).
- No page-wide animation when switching; a short colour transition at most.

## 6. Check both

- Look at every screen in both themes. The `preview` tool renders the page
  in its first theme at 360, 768 and 1440px, then at 1440px in the other
  theme, and reports text below 4.5:1 and drawings and icons below 3:1 in
  each.
- In the screenshots, look for what disappears or glares: line art, logos,
  chart lines, borders that shadows used to do the work of, white
  screenshots on dark.

## Avoid

Colours named for how they look; raw hex in components; `fill="#000"` or
`stroke="black"` in an SVG; one logo file for both themes; an SVG drawing in
an `<img>` on a two-theme page; inverted photos; `filter: invert(1)` on
anything but single-colour line art; a chart with light colours on dark;
shadows as the only edge on dark; a theme applied after the page paints; a
toggle with no name; a dark theme that is grey (the `ui` skill, neutrals).
