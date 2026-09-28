---
name: ui-audit
description: "Finding the marks of generated work in an interface that already exists: what to search the code for, how to judge each finding, and how to report and fix them. Read before reviewing, improving or redesigning an existing page or app."
---

# Auditing an existing interface

A page built by a model, or by hand from generated snippets, carries
recognisable marks. Most can be found in the code before the page is even
opened. Audit first, then change: fixing a page without knowing what is
wrong with it produces a different generic page.

## 1. How to audit

1. **Read the structure**: the page files, the components, the stylesheet or
   theme, `DESIGN.md` if any. Note the direction the project already has.
   An audit does not replace the owner's direction with yours.
2. **Search**: run the `ui_check` tool on the interface; it runs most of the
   searches in section 2 in one pass and lists the hits by priority. Use
   `grep` for anything it does not cover. Each hit is a candidate, not a
   verdict.
3. **Judge each candidate**: a technique is slop when it is there by default
   rather than for a reason (section 3). A gradient in the brand's own
   palette is fine; the default blue-to-purple on every section is not.
4. **Look at the rendered page** with the `preview` tool: overflow on a
   phone, contrast as rendered, dead links, controls without a name, small
   touch targets, console errors. Then the layout against the checks in the
   `ui-layout` skill: focal point, text measure, rhythm, phone layout.
5. **Report** (section 4), then fix what the task covers.

## 2. What to search for

Search the source for each pattern; the list is where generated work
usually shows.

**Colour and effects**
- Default gradients: `linear-gradient\(.*(purple|violet|indigo|#6366f1|#8b5cf6|#a855f7)`,
  `from-(purple|violet|indigo|blue)-[0-9]+ .*to-(purple|violet|pink|indigo)`,
  `bg-clip-text` with a gradient (gradient headline text).
- Glass everywhere: `backdrop-filter`, `backdrop-blur` on more than one or
  two elements.
- Glow: `box-shadow: 0 0 [0-9]+px`, `shadow-.*/(50|60|70)`, `drop-shadow`
  with a colour, on many elements.
- Heavy shadows by default: `shadow-(xl|2xl)` on most cards.
- Pills everywhere: `rounded-full` or `border-radius: 9999px` on buttons,
  inputs, cards and badges together.
- Decorative backgrounds: `radial-gradient` blobs, `bg-grid`, dot or grid
  patterns, `blur-3xl` orbs.
- Colours outside the tokens: `#[0-9a-fA-F]{3,8}`, `rgb\(` outside the
  token definitions; Tailwind arbitrary values `\[#`.

**Type**
- Default fonts chosen without a reason: `Inter`, `Geist`, `Space Grotesk`,
  `Poppins` with no design note.
- Decorative labels: `uppercase` together with `tracking-widest` or
  `letter-spacing: 0.2em` and more above headings.
- Monospace as costume: `font-mono` on headings.

**Copy**
- Buzzwords: `unlock|elevate|empower|seamless|revolutioni|next-gen|cutting-edge|supercharge|effortless|game-chang|world-class|powerful`
  (and the same words in the project's language).
- Generic actions: `>\s*(Get Started|Learn More|Try Now|Explore|Discover|Submit|Click here)\s*<`.
- Em dashes in interface copy: `\u2014` (search for the character itself).
- Emoji in interface text: characters in the emoji ranges inside JSX, HTML
  or strings rendered to the page (🚀 ✨ ⚡ 🔥 ✅ 💡 🎯).
- Placeholder text left in: `lorem ipsum`, `John Doe`, `jane@example`,
  `Acme`, `Your Company`.

**Fabricated content**
- Invented figures: `[0-9]+[kKmM]?\+`, `[0-9.]+%`, `99\.9`, `10x`, near words
  like users, customers, uptime, faster, rating.
- Testimonials, star ratings, "trusted by" logo rows: search for
  `testimonial`, `rating`, `stars`, `trusted`, `logos`. Real only with a
  source.

**Structure and behaviour**
- Dead controls: `href="#"`, `href=""`, `onClick={() => {}}`, buttons with no
  handler, nav links to anchors that do not exist.
- Removed focus: `outline: none`, `outline-none` without a `focus-visible`
  replacement.
- Fixed viewport heights: `100vh`, `h-screen` on sections.
- Fixed widths that break phones: `width: [0-9]{3,}px`, `w-\[[0-9]{3,}px\]`,
  `min-width` on text containers.
- Missing states: data fetching with no empty, loading or error branch.
- Div soup: clickable `div`s (`<div onClick`), `div`s where `button`, `a`,
  `nav`, `main` or `ul` belong; images with no `alt`.
- Repetition: the same card markup copied three or more times instead of a
  component; the same section composition repeated down the page.
- Icons: emoji as icons; more than one icon library imported
  (`lucide-react` and `react-icons` and `@heroicons` together); the generic
  set (sparkles, rocket, zap, wand) on feature cards.

## 3. Judging a finding

For each candidate ask: what does this serve? Keep it when the answer names
a purpose (the brand's colour, a real hierarchy, a real status). It is slop
when the only answer is "it looks modern" or there is no answer.

Rank what is left:

- **High**: dishonest or broken. Invented figures, testimonials or logos,
  dead controls, removed focus, contrast below AA, horizontal scroll on a
  phone, missing error states.
- **Medium**: generated look. Default gradients and glass, identical card
  grids, centred everything, buzzword copy, generic actions, emoji icons,
  one composition repeated.
- **Low**: consistency. Colours outside tokens, mixed icon sets, stray
  radii, duplicated markup that should be a component.

## 4. Reporting

Report findings as a numbered list, highest first, each with the file and
line, what it is, why it reads as generated or what breaks, and the fix in
one line:

```
1. HIGH  app/page.tsx:42  "10,000+ happy clients" has no source.
   Fix: remove the figure, or mark it [REAL FIGURE].
2. MED   components/Hero.tsx:12  purple-to-pink gradient headline, not in
   the brand palette. Fix: solid text colour from the tokens.
```

Then, when the task is to improve the interface, fix the high and medium
findings in scope, following the `ui`, `ui-layout`, `ui-page-*` and `ui-part-*`
skills, and say which you left and why. When the task is only to review,
change nothing.
