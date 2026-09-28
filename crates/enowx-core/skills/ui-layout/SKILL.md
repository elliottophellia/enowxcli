---
name: ui-layout
description: "Concrete layouts and positioning: pages versus application screens, the app shell and how it changes at each width, where the title, actions, filters, tables, panels and messages go, alignment lines, grid and spacing numbers, keeping everything inside the screen, and how each layout rearranges on a phone. Read before laying out a page or screen, or when a layout looks generic or off."
---

# Layout and positioning, with numbers

A generated layout is recognisable before any word is read: everything
centred, a narrow column floating in the middle of an app with empty bands
on both sides, every section in its own card, a filter box with a title of
its own, a sidebar that stays open on a phone, controls in every table row,
content running off the right edge. This skill gives the decisions a designer
makes, as numbers and positions. The `ui` skill says why; this says where and
how much.

## 1. Decide before placing anything

In order, before the first line of markup:

1. **The kind of screen**: a page people read (section 2a) or an
   application screen people work in (section 2b). The container rules
   differ.
2. **The job**: the one thing a visitor must do or learn here. One screen,
   one job: a second list or form that belongs to another job goes on its
   own screen or in a panel, not stacked below.
3. **The focal point**: the element that does that job, with the most size,
   contrast or space. Only one.
4. **The reading order**: first, second, third. The markup follows it; the
   layout follows the markup.
5. **The density**: pages breathe (large type, few things per screen);
   tools are dense (14px text, many rows, 8 to 16px gaps).

## 2. Two kinds of screen

### 2a. Pages (marketing, docs, articles, a business's site)

- A centred container: `max-width: 1200px` (1120 to 1280), side padding
  `clamp(16px, 5vw, 48px)`. Backgrounds may run full width; the content does
  not.
- Body text at `max-width: 65ch`.

### 2b. Application screens (inside a sidebar or a top bar)

- **The content is anchored to the shell, never floating in it.** It starts
  at the sidebar's edge plus the page padding (24 to 32px on wide screens,
  16px on phones) and runs to the right edge minus the same padding. No
  `mx-auto` on the page column: a centred `max-w-7xl` inside a shell leaves
  an empty band between the sidebar and the content that grows with the
  screen.
- **Width by content**: lists, tables and dashboards use the full width, up
  to about 1600px on very wide screens (then the extra space goes to the
  right). Forms, settings and detail pages keep a readable column of 640 to
  960px, **left-aligned** (`max-w-3xl`, with no `mx-auto`), so their left edge
  lines up with every other screen.
- **One left edge**: the page title, the toolbar, the table, the cards and
  the empty state all start on the same vertical line. Moving from one
  screen to the next, that line does not jump.

## 3. The measures

- **Grid**: 12 columns on wide screens, 24px gutter (16px on phones).
  Content spans columns: text 6 to 7, media 5 to 6, a side panel 3 to 4.
- **Spacing** (4px base, as tokens):
  - inside a component: 8 to 16px;
  - related elements (label and field, heading and its text): 8 to 12px;
  - groups inside a section or blocks on an app screen: 16 to 24px;
  - sections of a page: 64 to 96px wide, 48 to 64px on phones;
  - a heading sits closer to what follows than to what came before.
- **Type** (1.25 ratio): 14, 16, 20, 25, 31, 39, 49. Body 16 to 18px on a
  page, 14px in dense tools; h1 39 to 61px on a landing page, 20 to 28px on
  an application screen.
- **Heights**: controls 36 to 40px on desktop, 44px touch targets on phones;
  top bar 56 to 64px; table rows 40 to 48px.

## 4. Where each thing goes

- **Title and actions**: the page title (`h1`) at the top left; the one
  primary action on the same row at the top right, vertically centred with
  the title; secondary actions to its left as quieter buttons, or in a
  "More" menu past two. The right edge of the primary action is the right
  edge of the content below it. A description line under the title only
  when a first-time user needs it; never a sentence restating the title.
- **Filters and search**: a toolbar row directly above the list it filters,
  as wide as the list, with no card or heading of its own: search first
  (left, the widest control, 240 to 400px), then the filters, then view
  options or toggles at the right. The result count sits at the start of the
  table's footer or beside the toolbar, not as a sentence in a box. When
  rows are selected, the bulk actions replace the toolbar in the same place.
- **The list or table**: one surface holding the toolbar, the header row,
  the rows and the footer (count on the left, pagination on the right). Not
  a filter card, a gap, and a table card.
- **Record actions**: the daily one or two in the row as quiet buttons or
  links, the rest in a labelled menu at the row's end; editing a record opens
  a side panel from the right (400 to 560px) or its own page, not inputs in
  every row.
- **Forms**: labels above fields, fields in one column (two only for short
  paired fields such as city and postcode), the Save button at the end of the
  form, aligned with the fields' left edge or at the right of a footer bar,
  Cancel beside it; a destructive action apart, in a "Danger zone" at the end
  or in a menu, never next to Save.
- **Side panels and dialogs**: a panel slides from the right for detail and
  editing while the list stays visible; a dialog (480 to 640px, centred) only
  for a short decision or a small form. Their footers hold the actions,
  primary at the right.
- **Messages**: a toast at the bottom right (bottom centre on phones), above
  any sticky bar; an inline error beside the field or on the block that
  failed; an empty state in the middle of the region it replaces, not of the
  page.
- **Sticky parts**: only what the user needs while scrolling (the top bar, a
  long table's header, a bulk-action bar, a long form's Save bar), each with
  a solid background (`ui-page-dashboard`, section 2a).

## 5. The application shell

The frame most application screens sit in, and what it does at each width:

| Width | Navigation | Content |
|---|---|---|
| 1280px and up | Sidebar 224 to 256px, full height, sticky, its own scroll | Anchored beside it, padding 24 to 32px |
| 1024 to 1279px | The same sidebar, or a 64 to 72px icon rail with tooltips | Padding 24px |
| Under 1024px | No sidebar. A top bar 56px high, sticky, with a labelled "Menu" button that opens the same navigation as a drawer from the left | Full width, padding 16px |

A skeleton to start from (Tailwind; the same structure in any CSS):

```html
<div class="min-h-dvh lg:grid lg:grid-cols-[240px_minmax(0,1fr)]">
  <aside class="hidden lg:flex lg:flex-col sticky top-0 h-dvh border-r">
    <!-- product name, navigation, account at the bottom -->
  </aside>
  <div class="min-w-0">
    <header class="lg:hidden sticky top-0 z-30 flex h-14 items-center gap-3 border-b bg-background px-4">
      <!-- "Menu" button (icon and word) opening a drawer, the screen's title, its primary action -->
    </header>
    <main class="px-4 py-4 sm:px-6 lg:px-8 lg:py-6">
      <!-- the screen: full width for lists, max-w-3xl (not centred) for forms -->
    </main>
  </div>
</div>
```

- `minmax(0, 1fr)` and `min-w-0` let a wide table scroll inside its own
  container instead of pushing the whole page sideways.
- The drawer is a dialog: a scrim, focus moved into it and trapped, `Esc`
  and a close button to leave, closed after a link is chosen, focus back on
  the Menu button.
- On a phone the screen's primary action stays in view: in the top bar as a
  button with its word, or in a bar fixed to the bottom.
- The sign-in screen has no shell: a single centred column (`ui-page-sign-in`).

## 6. Page skeletons

Each kind of page has its own skill with the skeleton to start from:
`ui-page-landing`, `ui-page-local-business`, `ui-page-portfolio`,
`ui-page-docs`, `ui-page-dashboard`, `ui-page-list-detail`,
`ui-page-settings`, `ui-page-form`, `ui-page-sign-in`. Read the one for the
page you build. For a page people read, a worked example measured from a
real site sits beside them: `ui-reference-launch`, `ui-reference-saas`,
`ui-reference-studio`, `ui-reference-marketplace` (the `ui` skill says which
fits which page).

## 7. Section compositions (pages)

Pick per section by what it holds; do not repeat one down the page.

- **Split**: text 6 to 7 columns, media 5 to 6, aligned to the top or the
  middle of the text. Alternate sides only when the sections are parallel.
- **Label and content**: a label column (3 or 4 of 12) beside a content
  column (8 or 9 of 12).
- **Stacked**: heading, text at 65ch, then full-width media or a list.
- **List**: items as rows with a title and a line each.
- **Uneven grid**: one large item and several small ones, when one matters
  most.
- **Table or comparison**: when items share attributes to compare.
- **Quote or statement**: one line, large, lots of space around it.
- **Full-bleed band**: colour or image across the full width, content in the
  container, once or twice per page.

Headings and text are left-aligned. Centre only short standalone lines (a
closing call to action, a single statement), never paragraphs or lists.

## 8. Nothing wider than the screen

- No element wider than the viewport at any width from 360px: check the
  screenshot, and the `preview` report's overflow.
- Every flex or grid child that holds text or a table gets `min-width: 0`
  (`min-w-0`), so it can shrink.
- A wide table sits in its own scroll container (`overflow-x: auto` on a
  wrapper) with a visible edge and the first column sticky, or becomes cards
  below 640px (`ui-part-tables`). Lower-priority columns hide first
  (`hidden md:table-cell`).
- Short codes, prices, dates and badges do not wrap (`white-space: nowrap`);
  long names wrap, or truncate with the full text in a `title`.
- Button groups wrap onto a second line or fold into a menu; they never push
  past the edge.

## 9. On a phone

- One column, in reading order, not the wide-screen position.
- Side padding 16 to 20px; sections 48 to 64px apart on pages, blocks 16px
  apart on app screens; headings one step smaller.
- The app shell as in section 5: no sidebar, a top bar with "Menu", or a
  bottom bar of 3 to 5 labelled items for an app used on the phone all day.
- Toolbars: search stays; filters open in a sheet from one "Filters" button
  that shows how many are active.
- Tables become one card per record, or scroll in their container.
- The primary action within reach: in the top bar, at the end of the content,
  or sticky at the bottom in a long form.
- A back-to-top control once a page passes three screens
  (`ui-part-back-to-top`).

## 10. Check it

Look with the `preview` tool at 360, 768 and 1440px, on every screen you
built, including the ones behind a sign-in (give `preview` its `login`), and
fix what you see:

- Content that floats: an empty band between the sidebar and the content on
  a wide screen.
- A sidebar still open on a phone or a tablet, squeezing the content.
- Anything past the right edge, or a table cut off.
- A title or its primary action out of view at 360px.
- Everything centred; every section the same height and composition; a row
  of three equal cards with an icon on each.
- The same gap between everything, or headings with equal space above and
  below.
- Nothing on the screen clearly more important than the rest.
- A filter area in a card with its own heading; the list's count as a
  sentence in a box.
- Controls repeated in every row; the same badge on every row.
- Two jobs stacked on one screen.
- A wide-screen layout squeezed onto a phone rather than rearranged.
- A top bar that scrolls away, or a long page with no way back to the top.
