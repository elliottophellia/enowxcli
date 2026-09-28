---
name: ui
description: "Designing or restyling an interface: direction, layout, type, colour, icons, components, states, responsive behaviour. Read before building a page, a screen or a set of components."
---

# Interface design

Generated interfaces fail in a way anyone can spot: a gradient hero, three
identical feature cards, invented numbers, an icon on every line, everything
centred, nothing that could not belong to any other product. This is how to
build one that belongs to its product and works for the people using it.

## 1. Find the direction, or set one

Look before designing: `DESIGN.md`, a brand or style guide, the logo, the
colours and fonts already in the CSS or theme, screenshots in the repository.
Use what is there, including its tokens and components.

When there is nothing, set a direction from what the product is and who uses
it, and write it as one line in your report:

> Reading this as: a booking page for a neighbourhood physio clinic, calm and
> plain, one warm accent, no motion beyond hover.

Hold it on every screen. A direction you cannot state in a line is not one.

A mood and a palette are not yet a direction; they leave the page to fall
back on the default for its category. Give it a **concept**: one idea taken
from the subject itself that decides the layout, the type and the details.

- A developer who builds local-first command-line tools: the page as a
  well-made manual. Dense and typographic, sections set like a manual's
  headings, real commands with their real output as the images.
- A wedding photographer: the photographs full-bleed and in sequence, text
  small and sparse, the layout following the pictures' own proportions.
- A neighbourhood bakery: warm paper and a menu board's structure, prices in
  their own column, this week's bake as the headline.

Every category has a default that reads as generated. For a developer, a dark
page with monospace labels and one amber or green accent; for a startup, a
gradient hero over three cards; for a clinic, stock photographs of smiling
people; for a restaurant, a full-screen food photo under a script font. Name
the default for the category you are designing, then do something else, or
take it further on purpose because the concept asks for it.

Write it down in `DESIGN.md` at the project's root when the project has
none, so every later change keeps to it. Keep it short, decisions not
essays:

```markdown
# Design

Direction: a booking page for a neighbourhood physio clinic, calm and plain.
Concept: the clinic's own appointment card: one column, times in a grid.
Audience: adults booking a first visit, many on phones.
Theme: light only.
Palette: paper #F6F2E9, ink #15302A, muted #4C6058, accent #0F6B4F (the one
primary action), line #DCD4C4. Defined as tokens in styles.css.
Type: Fraunces for headings, Plus Jakarta Sans for text; scale 14/16/20/25/31/39.
Spacing: 4px base, tokens --s-1 to --s-9. Radii 6/12. Shadows only on overlays.
Icons: Tabler, outline, 20px beside text.
Motion: hover and focus only.
Placeholders: clinic name, phone, address, hours, prices (listed in README).
```

Read it before every change, and update it when the user changes a
decision.
Do not fall back to the generated default: a dark page, a blue-to-purple
gradient, glowing buttons and a grid background say "made by a model" and
nothing about the product.

## 2. Layout

- One focal point per screen: what the user came for is the strongest
  element, and everything else defers to it. One primary action per view;
  secondary actions look secondary.
- Structure follows the content and the task, not a template. A section
  exists because the product has something to say there. Shapes to avoid by
  default: hero, three identical cards, a "trusted by" logo row, testimonials,
  FAQ, CTA band, four-column footer, in that order on every page; "how it
  works" in exactly three numbered circles; a bento grid; a fake terminal
  window as the hero.
- Group by proximity: related things sit closer together than unrelated
  ones, and the space between sections is larger than the space inside them.
- Align to a grid: a few column widths, one gutter, edges that line up. Body
  text at 60 to 75 characters a line. Page content inside a maximum width
  (around 1100 to 1280px) while backgrounds may run full width.
- Vary the rhythm: not every section is a centred heading over a grid of
  equal cards. Alternate text-led and image-led, contained and full-width,
  dense and open.
- App screens: build around the decision the user makes there. The default
  admin shell (sidebar, four stat cards, a chart, a table) fits every product
  and so none. In tables, choose columns from that decision, put the deciding
  field first, left-align text and right-align numbers.

## 3. Spacing and sizing

- A spacing scale as tokens, on a 4px base (4, 8, 12, 16, 24, 32, 48, 64,
  96), used everywhere. No one-off values.
- Heights come from content. No fixed heights on anything holding text, no
  `100vh` sections; use `min-height` with `dvh` when a section really should
  fill the screen.

## 4. Typography

- One or two families, chosen for the product's character, with the reason
  in your report. A default pick (Inter, Geist, Space Grotesk, IBM Plex)
  made out of habit is a tell, not a choice.
- Give the headline face character: a serif, a grotesk with a distinct
  shape, a condensed or a wide face, whatever the concept asks for; pair it
  with a plain face for text. Monospace is for code, commands and figures
  that align, not a costume for every label.
- Contrast in size carries the hierarchy: the largest headline at least two
  and a half times the body size, the small print clearly smaller. A page
  where headings, text and meta are all within a few pixels reads flat.
- A type scale with few steps, as tokens (for example 14, 16, 20, 25, 31, 39:
  a 1.25 ratio), with `clamp()` for display sizes so they scale with the
  window.
- Body at 16px or more, line height about 1.5 for body and 1.1 to 1.25 for
  headings.
- Hierarchy through size and weight first, colour second. Avoid decorative
  uppercase labels with wide letter-spacing, and monospace headings used as
  a "tech" costume.

## 5. Colour

- Every colour is a token: background, surface, text, muted text, border,
  accent, and the status colours.
- A small palette: neutrals, two or three core colours and one accent. The
  accent marks the key moment (the primary action, the current item), not
  everything.
- Contrast is computed, not judged by eye: text 4.5:1, large text (24px, or
  19px bold) 3:1, controls, borders that identify a control, and focus
  indicators 3:1 against what surrounds them.
- Dark by default only with a reason the concept gives (a media player, a
  photographer of night scenes). "It is for developers" is not a reason on
  its own. Otherwise light, or a toggle where both themes are checked.
- Gradients, glass, glow and large shadows are accents with a purpose, on
  one or two elements, never the page's texture.

### Neutrals decide whether it looks designed

Most generated palettes fail in the neutrals, not the accent: a "dark"
page in charcoal grey (`#1e1e1e`, `#27272a`), a light page in dull grey
(`#e5e5e5`), text in the same grey family, everything a little muddy.

- **Dark means dark.** The page background at 3 to 8% lightness: `#0a0a0a`,
  `#0c0d0f`, `#111110`. Not `#1a1a1a` to `#2d2d2d`, which reads as a grey
  panel, not a dark room. Raised surfaces step up by 2 to 4% each
  (`#0c0d0f` page, `#141518` card, `#1c1d21` popover), so depth comes from
  small steps, not from a grey base.
- **Text on dark is off-white, not grey:** body at 88 to 94% lightness
  (`#ecebe8`), muted text around 60 to 65% (`#9a9893`), never below 4.5:1.
  Pure `#ffffff` body text on near-black glares; keep pure white for the
  one thing that must pop.
- **Light means light.** The page at 93 to 100% lightness (`#ffffff`,
  `#fafaf7`, a warm paper `#f6f3ec`), text near-black at 8 to 15%
  (`#16150f`), not mid-grey `#555` for body copy.
- **Tint the neutrals** a few degrees toward the accent's hue or the
  concept's temperature (warm ink `#12110f`, cool night `#0b0d12`) instead
  of dead RGB greys; keep the tint subtle (saturation 3 to 10%).
- **Borders are quiet:** 8 to 14% above the background on dark, 8 to 12%
  below it on light, and a border that marks a control reaches 3:1.
- **The accent is saturated enough to be the accent** against those
  neutrals, and used on under a tenth of the screen.
- The `preview` tool reports a page background in the grey middle ground
  (between about 9% and 92% lightness).

## 6. Icons

- One icon set for the whole product. Use the project's (look in its
  dependencies and assets). With none, choose one whose style suits the
  product (stroke weight, outline or filled, corner shape) and name it in
  your report. For example: Phosphor (six weights, friendly), Tabler (even
  2px stroke, technical, very large), Heroicons (compact, outline and solid),
  Material Symbols (variable weight and fill, suits app interfaces), Lucide
  (thin rounded strokes, so common it reads as a default: choose it for its
  look, not out of habit).
- Import icons one by one from the package, so only those used ship. A static
  page without a bundler inlines the SVGs it uses, as `<symbol>` elements
  referenced with `<use>`, fetched exactly with the `icon` tool: never type
  an icon's path from memory. No icon fonts, and no runtime icon script from a
  CDN for a handful of icons.
- Size icons on a scale tied to the text beside them (16, 20, 24), colour
  them with `currentColor`, keep one stroke width, centre them on the line.
- An icon earns its place by making something faster to recognise. A word is
  often clearer; not every list item or card needs one. Never emoji as
  interface icons, and not the generic glyphs (sparkles, rocket, lightning,
  magic wand) standing in for features.
- Icon-only buttons get an accessible name (`aria-label` or visually hidden
  text); decorative icons get `aria-hidden="true"`.

## 7. Components

- Each part of a page (header, navigation, sidebar, hero, sections, footer,
  buttons, forms, tables, dialogs, menus, notifications) has its own entry,
  with what to build and what to avoid, in its own `ui-part-*` skill
  (`ui-part-header`, `ui-part-hero`, `ui-part-tables`...), and each kind of
  page its skeleton in a `ui-page-*` skill. Read only the ones you build.
- Reuse first: search the project for the component (Button, Input, Dialog,
  Card, Tabs) and use or extend it.
- A component for each named concept, and wherever the same markup repeats
  with different content. Variants through props (`variant`, `size`), never
  copies. Pass native attributes through, so a Button stays a button.
- Design every interactive state: hover, focus-visible, active, disabled,
  loading, and for fields, invalid with a message.
- Radii from a small set (for example 4, 8, 12); shadows only to show that
  something sits above something else. A card only when the content is a
  card: a bordered box around every paragraph is noise, not structure.

## 8. States and behaviour

- Every view that shows data has an empty, a loading and an error state, and
  each says what is going on and what to do: "No invoices yet. Create one to
  see it here." A bare spinner or "No data" says nothing.
- Every control does something real: links go to pages or sections that
  exist, buttons act, forms submit and confirm. A control that cannot work
  yet is removed, or visibly labelled "Coming soon".
- A placeholder reads as a placeholder ("[your email]"). The note on how to
  fill it in goes in the README, never on the page where visitors read it.
- Forms: a visible label for each field, the error beside the field in words,
  a submit button that says what it does and shows progress while it works.
- Long pages: the top bar sticks, so the navigation is always in reach
  (`ui-part-header`), and a page longer than about three screens on a phone
  has a back-to-top control that appears after the first screen
  (`ui-part-back-to-top`).

## 9. Responsive

- Design the narrow screen as a state of its own, not a leftover: write the
  CSS mobile-first, then widen.
- Put breakpoints where the content breaks, not at device widths.
- Grids with `repeat(auto-fit, minmax(…, 1fr))` or a collapse to one column;
  `min-width: 0` on grid and flex children that hold text; images with
  `max-width: 100%` and their width and height set.
- No horizontal scroll at 360px. A wide table reflows or scrolls inside its
  own container.
- Touch targets at least 44px, with space between them; anything revealed on
  hover also opens on tap.
- Navigation collapses to a labelled menu, or a bottom bar that never covers
  the content.

## 10. Accessibility

- Semantic elements: `button` for actions, `a` for navigation, one `h1` and
  headings in order, `nav`, `main`, `footer`, lists for lists, `label` for
  every input.
- Keyboard: everything reachable in visual order and operable with Enter and
  Space; dialogs close with Escape and give focus back. A visible
  `:focus-visible` style on every control; never `outline: none` without a
  replacement.
- Colour is never the only signal: status colours come with text or an icon.
- Respect `prefers-reduced-motion`. Motion has a purpose (feedback,
  orientation, continuity), lasts 150 to 300ms, and nothing loops forever
  without one.
- Images have alt text that says what they show, or `alt=""` when they are
  decoration.

## 11. Content

- Real content, or placeholders that say they are placeholders:
  `[Company logo]`, `[Customer quote]`. Never invented statistics,
  testimonials, customer logos, people or awards.
- Facts about the business you were not given (its name, prices, hours,
  policies) stay visible placeholders on the page, not plausible guesses,
  and are listed in your report.
- Copy follows the `writing` skill: specific, plain, and actions named for
  what they do.

## 12. Before you call it done

- Build it, and run the project's linter and tests.
- Search the CSS for colour, spacing and radius values written outside the
  token definitions, and move each into a token.
- Look at it with the `preview` tool: it renders the page at 360, 768 and
  1440px and measures overflow, contrast, dead links, unnamed controls and
  touch targets, with a screenshot of each width. Without a browser, read
  the CSS for those widths instead, and say so.
- Walk the keyboard path and every state.
- A favicon, or `<link rel="icon" href="data:,">`, so the browser's request
  for one does not end in a 404 in the console.
- Report the direction, the icon set, and anything left as a placeholder.
