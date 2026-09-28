---
name: ui-layout
description: "Concrete layouts: the skeleton for each kind of page, grid and width numbers, spacing rhythm, section compositions, and how each collapses on a phone. Read before laying out a page or screen, or when a layout looks generic or off."
---

# Layout, with numbers

A generated layout is recognisable before any word is read: everything
centred, every section the same height, a grid of three equal cards, content
running the full width of a wide screen, the same 80px gap between
everything. This skill gives the decisions a designer makes, as numbers and
skeletons to start from. The `ui` skill says why; this says how much and
where.

## 1. Decide before placing anything

Write these down in your head, in order, before the first line of markup:

1. **The page's job**: the one thing a visitor must be able to do or learn.
2. **The focal point**: the element that does that job. It gets the most
   size, contrast or space on the screen. Only one.
3. **The reading order**: what is seen first, second, third. Structure the
   markup in that order; the layout follows it.
4. **The density**: marketing pages breathe (large type, few things per
   screen); tools are dense (small type, many things per screen). Pick one
   per page.

## 2. The measures

- **Page container**: `max-width: 1200px` (1120 to 1280), centred, with
  side padding `clamp(16px, 5vw, 48px)`. Backgrounds may run full width; the
  content does not.
- **Reading column**: body text at `max-width: 65ch` (60 to 75 characters).
  Never a paragraph across 1200px.
- **Grid**: 12 columns on wide screens with a 24px gutter (16px on phones).
  Content spans columns: text 6 to 7 of 12, a media block 5 to 6, a
  sidebar 3 to 4.
- **Spacing rhythm** (4px base, as tokens):
  - inside a component: 8 to 16px
  - between related elements (label and field, heading and its text):
    8 to 12px
  - between groups inside a section: 24 to 32px
  - between sections: 64 to 96px on wide screens, 48 to 64px on phones
  - heading to the content it introduces: less space than the space above
    the heading, so the heading belongs to what follows
- **Type scale** (1.25 ratio): 14, 16, 20, 25, 31, 39, 49. Body 16 to 18px;
  h1 on a landing page 39 to 61px (`clamp()`), in an application 25 to 31px.

## 3. Page skeletons

Each kind of page has its own skill with the skeleton to start from:
`ui-page-landing`, `ui-page-local-business`, `ui-page-portfolio`,
`ui-page-docs`, `ui-page-dashboard`, `ui-page-list-detail`,
`ui-page-settings`, `ui-page-form`, `ui-page-sign-in`. Read the one for the
page you build.

## 4. Section compositions

Pick per section by what it holds; do not repeat one down the page.

- **Split**: text 6 to 7 columns, media 5 to 6, aligned to the top or the
  middle of the text. Alternate sides between sections only when the
  sections are parallel.
- **Stacked**: heading, text at 65ch, then full-width media or a list below.
- **List**: items as rows with a title and a line each; right for features,
  steps and anything read in order.
- **Uneven grid**: one large item and several small ones, when one matters
  most.
- **Table or comparison**: when items share attributes to compare.
- **Quote or statement**: one line, large, lots of space around it.
- **Full-bleed band**: a colour or image across the full width, content in
  the container, used once or twice per page to change pace.

Centring: headings and text are left-aligned by default. Centre only short
standalone lines (a closing call to action, a single statement), never
paragraphs or lists.

## 5. On a phone

- One column. Order follows reading order, not the wide-screen position.
- Side padding 16 to 20px; section spacing 48 to 64px; type one step
  smaller for headings.
- Media full width of the container; a split becomes text then media.
- Navigation behind a labelled "Menu" button, or a bottom bar of 3 to 5
  labelled items in an application.
- Tables: a scroll container with a visible edge, or one card per row.
- The primary action within reach: full width at the end of the content, or
  sticky at the bottom in a long form.
- Check with the `preview` tool, which renders the page at 360px (and 768
  and 1440): no horizontal scroll, no tap target under 44px, and in the
  screenshot no text touching the edges.

## 6. Checks for a layout that looks generated

Before calling a layout done, look for these and fix them:

- Everything centred.
- Every section the same height and the same composition.
- A row of three equal cards with an icon on top of each.
- Text running wider than 75 characters.
- The same gap between everything (no difference between inside a group and
  between groups).
- Headings with equal space above and below.
- Nothing on the screen clearly more important than the rest.
- A wide-screen layout squeezed onto a phone rather than rearranged.
