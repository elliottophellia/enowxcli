---
name: frontend-accessibility
description: "Accessibility in code: semantic HTML first, the ARIA patterns for custom widgets, keyboard and focus management, accessible names, live regions, forms, media, colour, zoom and motion, and testing with axe, a keyboard and a screen reader. Read before building any interactive component, and before calling a page done."
---

# Accessibility in code

The generated interface is built from `div`s with click handlers, icon
buttons with no name, a dropdown that opens on click but never from the
keyboard, a dialog that lets focus wander behind it, `outline: none`
everywhere, and ARIA attributes sprinkled where they change nothing or
make things worse. It passes a glance and fails the first keyboard or
screen reader user. This is the code that makes an interface work for
everyone: native elements first, the ARIA patterns where a custom widget
is needed, focus handled on purpose, and a pass by hand. The visual rules
are in `ui`; the whole frontend in `frontend`.

## 1. The bar

- WCAG 2.2 level AA on every screen. It is also what the law points to in
  many places: the European Accessibility Act applies to many products and
  services sold in the EU from 28 June 2025, through EN 301 549.
- New in 2.2 and often missed: focus not hidden behind sticky bars
  (2.4.11), a single-pointer alternative to dragging (2.5.7), targets of at
  least 24 by 24px (2.5.8), help in the same place on every page (3.2.6),
  nothing asked twice (3.3.7), and sign-in without a memory or puzzle test,
  with paste and password managers allowed (3.3.8).
- The WebAIM Million survey of home pages finds the same failures every
  year: low-contrast text, images without alt text, fields without labels,
  empty links, empty buttons, no page language. All six are cheap to
  prevent.

## 2. Semantic HTML first

| Need | Use | Not |
|---|---|---|
| An action | `<button type="button">` | `<div onClick>`, `<a href="#">` |
| Going somewhere | `<a href="/orders">` | a button that calls `navigate()` |
| Regions | `header`, `nav`, `main` (one), `aside`, `footer` | `div`s with class names |
| Structure | one `h1`, then `h2` and `h3` in order | headings chosen for their size |
| Lists and data | `ul`, `ol`, `dl`; `table` with `th scope` and a `caption` | a `div` per item or cell |
| Show and hide | `details` and `summary`, or a button with `aria-expanded` | a clickable heading |
| A modal | `dialog` opened with `showModal()` | a `div` over a scrim |

- A `<div onClick>` lacks focus, Enter and Space, a role, a disabled state
  and form submission; adding them back (`role`, `tabIndex`, key handlers)
  is more code than the `button` it imitates.
- The first rule of ARIA: when a native element does the job, use it. No
  ARIA is better than wrong ARIA, and pages with ARIA average more errors
  than pages without. ARIA changes what is announced, never behaviour: a
  `role="button"` still needs its keys.
- Several `nav`s are told apart with `aria-label` ("Primary",
  "Breadcrumb"). Never `aria-hidden="true"` on anything focusable or its
  parent.

## 3. Names and descriptions

- Every control has an accessible name: its visible label (`<label>`,
  button text), else `aria-labelledby` pointing at visible text, else
  `aria-label`. A `title` is not a label.
- Icon-only buttons: `aria-label="Delete invoice"`, or visually hidden
  text. Repeated actions name their object: twenty "Edit" buttons become
  "Edit INV-204".
- The visible label starts the accessible name, so voice control ("click
  Send") works (2.5.3).
- Hints and errors are descriptions, tied with `aria-describedby`.
- Alt text by the image's job:

| Image | `alt` |
|---|---|
| Carries content (a photo that matters, a simple chart) | what it shows that matters here, in a sentence |
| Inside a link or button | where it goes or what it does ("Home", "Search") |
| Decorative, or repeated by the text beside it | `alt=""` |
| Complex (a diagram, a detailed chart) | a short `alt`, the full description in the text or a `figcaption` |
| An image of text | the text |
| A logo | the organisation's name |

- Never "image of", never a file name; with no `alt` at all, screen
  readers read the file name. Decorative SVGs get `aria-hidden="true"`; a
  meaningful one gets `role="img"` and `aria-label`.
- Text hidden on screen but read aloud:

```css
.sr-only {
  position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
  overflow: hidden; clip-path: inset(50%); white-space: nowrap; border: 0;
}
```

## 4. Keyboard

- Everything a pointer can do, a keyboard can: Tab and Shift+Tab move
  between controls in DOM order; Enter activates links and buttons; Space
  activates buttons, checkboxes and switches.
- DOM order is visual order: CSS `order`, grid placement and absolute
  positioning never make focus jump around the screen.
- A composite widget (tabs, menu, listbox, radio group, toolbar, grid) is
  one tab stop, with arrow keys inside: a roving `tabindex` (`0` on the
  current item, `-1` on the rest) or `aria-activedescendant`.
- Escape closes the top layer: a menu, popover, dialog or drawer.
- No traps: focus can always leave a component (a modal keeps it on
  purpose, and Escape leaves).
- `tabindex` above 0: never. `tabindex="-1"` only on elements focused from
  code (a heading after navigation, an error summary).
- What shows on hover also shows on focus, stays while the pointer moves
  onto it, and hides with Escape (1.4.13).
- Dragging (reordering, sliders, maps) has a click or key alternative:
  "Move up" and "Move down", arrow keys (2.5.7).
- Single-key shortcuts can be turned off or remapped (2.1.4), and never
  take keys that screen readers and browsers use.

## 5. Focus management

- A visible focus style on every control, never removed without a
  replacement: an outline (it survives Windows forced colours, where
  shadows vanish), at least 2px, with 3:1 contrast against its
  surroundings:

```css
:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
```

- Dialogs and drawers: focus moves in on open (to the first field, or the
  dialog when it opens with text to read), stays inside (`showModal()`
  makes the rest inert; otherwise `inert` on the background), and returns
  to the opener on close. If the opener is gone (its row was deleted), to
  the next row or the list's heading.
- Client-side route changes are announced and focus starts from the new
  page. Next.js and SvelteKit announce the new page themselves; with a
  router that does not, move focus to the new `h1` (with `tabIndex={-1}`)
  after every navigation but the first. Keep `document.title` current.
- A skip link as the first focusable element, visible on focus:
  `<a href="#main" class="skip-link">Skip to content</a>`, with
  `<main id="main" tabindex="-1">`.
- Sticky bars never cover the focused element: `scroll-padding-top` (and
  bottom) on the scroller, the bar's height plus 8 to 16px (2.4.11).
- When an action removes what had focus (a deleted item, a finished step),
  put focus somewhere meaningful, never on `document.body`. Content that
  loads never steals focus; it is announced (section 7).

## 6. Custom widgets: the APG patterns

Take a tested implementation before writing one: Radix Primitives, React
Aria, Headless UI, Ark UI, Base UI, Reka UI (Vue), Bits UI and Melt UI
(Svelte), Angular CDK, or the project's library built on one (`ui`). When
you must build one, follow the WAI-ARIA Authoring Practices pattern:

| Widget | Roles and states | Keys |
|---|---|---|
| Modal dialog | `dialog`, or `role="dialog"` with `aria-modal="true"`; `aria-labelledby` its title | Tab cycles inside; Escape closes; focus returns |
| Disclosure | `button` with `aria-expanded`, `aria-controls` | Enter and Space toggle |
| Accordion | each header a `button` inside a heading, with `aria-expanded` | Enter, Space; optional Up, Down, Home, End between headers |
| Tabs | `tablist`; `tab` with `aria-selected`, `aria-controls`; `tabpanel` | Left and Right move (and select, unless manual); Home, End; Tab enters the panel |
| Menu button | `button` with `aria-haspopup="menu"`, `aria-expanded`; `menu`, `menuitem` | Enter, Space or Down open on the first item, Up on the last; arrows, Home, End, type-ahead; Escape closes and returns focus; Tab closes |
| Listbox | `listbox`; `option` with `aria-selected` | Up, Down, Home, End, type-ahead; Space toggles in multi-select |
| Combobox | input with `role="combobox"`, `aria-expanded`, `aria-controls`, `aria-activedescendant` | Down opens and moves; Enter picks; Escape closes, then clears; Alt+Down opens |
| Tooltip | `role="tooltip"`, referenced by `aria-describedby` | shows on hover and focus; Escape hides |
| Switch | `button` with `role="switch"`, `aria-checked` | Space toggles (Enter too, on a button) |
| Slider | `input type="range"`, or `slider` with `aria-valuenow`, min, max, `aria-valuetext` | arrows by a step; Page Up and Down by more; Home, End |
| Radio group | native radios in a `fieldset` | arrows move and select; one tab stop |
| Grid | `grid`, `row`, `gridcell`, only when cells are interactive | arrows between cells; Home, End; Ctrl+Home, Ctrl+End; one tab stop |
| Carousel | each slide a labelled group ("3 of 5"); previous, next, pause | rotation stops on hover and focus |

- A `menu` is for application commands. Site navigation with sub-pages is a
  disclosure holding links, not a menu.
- Native first again: `<select>` before a custom listbox, `<input
  type="range">` before a custom slider, `<details>` before a hand-made
  disclosure. A table people only read is a `table`, not a `grid`.
- Tooltips hold no links or buttons (use a popover or dialog), and are
  never the only place a control's name lives.

## 7. Live regions

- Changes the user did not move to are announced from a live region that
  is in the DOM before its text changes; a region inserted together with
  its text is often not read.
- `role="status"` (polite) for results counts ("24 invoices match"),
  "Saved", toasts and loads finishing; `role="alert"` (assertive) only for
  errors that need attention now.
- Short messages, one at a time; debounce them while someone types. Toast
  libraries (Sonner and the like) set this up; check they announce.

```tsx
// Always mounted; only its text changes
<p role="status" className="sr-only">{resultsMessage}</p>
```

## 8. Forms, media, colour and zoom

- **Forms**: a label on every field, `fieldset` and `legend` for groups,
  errors tied with `aria-describedby` and `aria-invalid`, required shown
  in text, `autocomplete` tokens (1.3.5), paste allowed in passwords and
  codes (3.3.8), nothing asked twice ("Same as billing address", 3.3.7).
  The behaviour is `frontend-forms`.
- **Media**: captions on video with speech (`<track kind="captions">`), a
  transcript for audio, a description of what the picture shows that the
  sound does not; no autoplay with sound; a pause for anything that moves
  by itself for more than 5 seconds (2.2.2); nothing flashing more than 3
  times a second (2.3.1).
- **Contrast**: 4.5:1 for text, 3:1 for large text (24px, or 18.66px
  bold), 3:1 for control edges and states, meaningful icons, focus
  indicators and chart marks; measured in both themes (`ui`, `ui-themes`).
- **Not colour alone**: links in running text are underlined; errors,
  statuses and chart series carry text, an icon or a pattern.
- **Zoom and reflow**: at 320px wide (1280px at 400%) nothing needs
  scrolling in two directions except tables and maps; text at 200% still
  fits; a reader's text spacing (line height 1.5, letter spacing 0.12em)
  breaks nothing, so no fixed heights on text (1.4.10, 1.4.4, 1.4.12).
  Font sizes in `rem`; never `maximum-scale=1` or `user-scalable=no`.
- **Targets**: at least 24 by 24px with spacing (2.5.8), 44px on touch
  screens (`ui-layout`).
- **Forced colours** (`@media (forced-colors: active)`): borders and
  outlines stay, backgrounds and shadows go; icons use `currentColor`, and
  no state is only a background colour.
- **Motion**: reduced motion and flashing are `motion-comfort`.
- **Language and titles**: `<html lang>` set from the active locale
  (`i18n`), `lang` on phrases in another language, `dir="rtl"` for Arabic
  and Hebrew with logical properties (`margin-inline-start`, `text-align:
  start`); a unique `<title>` per page, the page first, then the product.

## 9. Testing

- Lint: `eslint-plugin-jsx-a11y`, `eslint-plugin-vuejs-accessibility`,
  Svelte's compiler warnings (never silenced), the `@angular-eslint/template`
  accessibility rules.
- Component tests query by role and name (`getByRole("button", { name:
  "Save" })`): a control a test cannot find that way is a bug. Add
  vitest-axe or jest-axe; jsdom has no layout, so contrast is not checked
  there (`frontend-testing`).
- End-to-end: `@axe-core/playwright` on each key screen and state (a
  dialog open, errors shown):

```ts
const results = await new AxeBuilder({ page })
  .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
  .analyze();
expect(results.violations).toEqual([]);
```

- By hand, every time, because tools find only part of the problems: not
  whether alt text is right, focus order makes sense, announcements happen
  or a custom widget works.
  - Keyboard: put the mouse away and do the task. Focus always visible,
    order logical, nothing unreachable, Escape closes, focus returns.
  - Zoom to 200%, and a 320px wide window: nothing lost or overlapping.
  - A screen reader for 5 minutes: VoiceOver on macOS (Cmd+F5; VO+U opens
    the rotor with headings, links, landmarks and form controls) or NVDA
    with Firefox or Chrome on Windows (H headings, D landmarks, F form
    fields). Do the main task; listen for names, states, announcements.
- Chrome DevTools, Elements, Accessibility pane: each element's computed
  role and name.

## Check it

- `preview` at 360, 768 and 1440px and in the second theme: text below AA
  contrast, icons below 3:1, images without alt, controls without a name,
  touch targets under 44px, the number of `h1`s.
- `ui_check`: removed focus styles, images without alt, dead controls;
  `grep -rnE "<(div|span)[^>]*onClick" src` for clickable `div`s.
- axe clean in component and end-to-end tests; the keyboard and screen
  reader passes above on the screens you built.

## Avoid

Clickable `div`s and `href="#"` buttons; icon buttons with no name;
`outline: none` with nothing in its place; ARIA roles without the keys
they promise; `role="menu"` for site navigation; `tabindex` above 0;
dialogs that let focus escape or never return it; focus lost to the body
after a route change or a delete; live regions inserted with their text;
placeholder-only fields; colour as the only signal; zoom disabled; a page
called done without a keyboard pass.
