---
name: ui-components
description: "How to build each part of an interface without the generated look: header, navigation, sidebar, hero, sections, footer, buttons, forms, tables, dialogs, tabs, menus, notifications, and more. Read before building or reworking one of them."
---

# Components, one by one

Each entry says what the part is for, how to build it, and the generated
version to avoid. The principles (direction, spacing, type, colour, icons,
states, accessibility) are in the `ui` skill; this is where they meet a
specific part. Build only the parts the product needs.

## Page frame

### Header (top bar)

- Use it for: saying where the user is and reaching the few places they go
  most. Not for everything the site has.
- Build:
  - Logo or product name on the left, linking home. The main navigation
    next, then one primary action on the right, if the product has one.
  - 56 to 72px tall on wide screens, 56px on a phone. A solid background,
    and a border or a shadow only once the page has scrolled under it.
  - Sticky only on long pages where navigation is used mid-page; on a phone
    let it scroll away, or shrink it.
  - A "Skip to content" link as the first thing a keyboard reaches.
  - The current page marked with `aria-current="page"` and more than colour
    (weight or an underline).
  - On a phone the links collapse into a button labelled "Menu", not an
    unlabelled icon.
- Avoid: a glass header with a glow, seven links to pages that do not exist,
  "Log in" beside a gradient "Get started" pill, a "Beta" badge next to the
  logo.

### Navigation

- Use it for: three to seven destinations, named with the nouns users
  already use ("Invoices", "Clients", not "Solutions").
- Build:
  - Order by how often each place is used, not by the org chart.
  - Dropdowns only for real groups. They open on click, tap and Enter, close
    on Escape and on a click outside, and never exist only on hover.
  - The phone menu is a full-width panel or sheet with 44px rows, a labelled
    close button, focus kept inside while open, and the page behind it
    locked from scrolling.
  - An app with three to five main places on a phone can use a bottom tab
    bar: an icon and a label on every tab, the current one marked, and the
    content padded so the bar never covers it.
- Avoid: mega-menus for a site with ten pages, icons without labels, a
  hamburger on wide screens where the links fit.

### Sidebar

- Use it for: an application with more sections than a top bar holds, where
  users move between them all day.
- Build:
  - 240 to 280px wide, grouped under short headings, the current item
    marked. At most two levels deep.
  - Account and settings at the bottom, the daily work at the top.
  - It may collapse to icons on wide screens: each icon then has a tooltip,
    and expanding brings the labels back.
  - On narrow screens it becomes a drawer opened by a labelled button, with
    the same focus rules as a dialog.
- Avoid: a sidebar on a marketing site, every item a different coloured
  icon, a user card with an invented name and avatar, nesting three levels.

### Page header (inside an app)

- Use it for: the title of the current screen and its one primary action.
- Build: the title as the page's `h1`, a short description only if it helps
  a first-time user, the primary action on the right, secondary actions
  quieter or in a menu, breadcrumbs above when the hierarchy is deep.
- Avoid: a row of five equal buttons, a title that repeats the navigation
  label with nothing added.

### Footer

- Use it for: what people look for at the end: contact, address and hours
  for a local business, legal pages that exist, secondary links.
- Build: as many columns as there are real groups of links, often one or
  two. The owner's real name in the copyright line, or a placeholder.
- Avoid: the four-column Product / Company / Resources / Legal template,
  social icons for accounts that do not exist, a newsletter form that sends
  nothing.

## Page content

### Hero

- Use it for: saying in one sentence what this is and for whom, and offering
  the one next step.
- Build:
  - A headline that states the product or the benefit in the reader's words,
    under about ten words, as the page's only `h1`.
  - One supporting line with the detail that makes the headline credible.
  - One primary action named for what it does ("Book a session"), and at
    most one quieter secondary action.
  - A real visual: a screenshot of the product, a photograph of the place or
    the work, or none. Left-aligned text often reads better than centred
    once it runs past one line.
  - Height from the content, not `100vh`.
- Avoid: "Welcome to…", "Unlock the power of…", gradient text, abstract
  blobs or 3D shapes, a floating "New" pill, a row of invented numbers under
  the buttons, a "trusted by" logo row, a fake terminal window.

### Sections

- Use them for: one idea each, in the order the reader needs them.
- Build: a heading that says the point of the section ("Booking takes two
  messages") rather than labels it ("How it works"); content shaped by what
  it is: prose, a list, a table, an image with a caption, steps.
- Avoid: an uppercase eyebrow over every heading, every section a centred
  heading above a grid of equal cards.

### Feature lists and cards

- Use cards for: parallel, self-contained items a user compares or picks
  from. A list with a short line each is usually clearer.
- Build:
  - Emphasis follows importance: the main feature can be larger, or shown
    with a screenshot, and the rest listed.
  - A card that is a link is one link: the title's link stretched over the
    card, never a button nested inside a link.
  - Equal heights come from the grid, content aligned to the top.
  - An icon only when it helps recognition, from the product's one set.
- Avoid: three identical cards with an icon in a coloured circle each, a
  card around every paragraph, "Lightning fast / Secure / Scalable".

### Testimonials, logos and numbers

- Use them for: evidence that exists: a quote with the person's real name
  and permission, logos of real customers who agreed, figures with a
  source.
- Build: the quote in the person's words with their name and role; a number
  with its source or its date.
- Avoid: all of them when they are not real. Leave the section out, or mark
  a placeholder: `[Customer quote]`.

### Pricing

- Use it for: letting a user choose a plan and know what they pay.
- Build:
  - As many plans as are real. Features in the same order in every plan so
    they can be compared.
  - The price with its currency and period, and what the button does next.
  - One plan highlighted only when there is a reason to recommend it, with
    the reason in words.
- Avoid: three columns by default, "Most popular" on the middle plan with no
  basis, crossed-out prices that were never charged.

### FAQ

- Use it for: questions real users ask, answered briefly.
- Build: `<details>` and `<summary>`, or an accordion with `aria-expanded`;
  one question per item; answers of a few sentences with links onward.
- Avoid: template questions that fit any product ("Is my data secure?"). No
  known questions means no FAQ.

### Call to action band

- Use it for: repeating the primary action at the end of a long page.
- Build: one line on why, one button with the same verb as the hero's.
- Avoid: more than one per page, a new slogan in it.

## Controls

### Buttons

- Build:
  - One primary button per view; secondary and text buttons below it in
    weight.
  - The label is a verb and an object: "Create invoice", "Send reminder".
  - A `button` element (`type="button"` unless it submits), at least 44px
    tall to the touch, padding from the spacing scale.
  - Loading: a spinner inside, the width kept, `aria-busy="true"`, clicks
    ignored. Disabled: say why near it, or leave it enabled and explain on
    click.
  - A destructive action looks it (the danger colour, a confirming step).
- Avoid: an arrow on every button, pill plus gradient plus glow as the
  default look, "Submit", "Click here", two primaries side by side.

### Links

- Build: underlined in running text, or distinct by more than colour; a
  visible focus style; a new tab only when leaving mid-task would lose work,
  with `rel="noopener"` and a hint that it opens a new tab.
- Avoid: buttons styled as links for navigation, links styled as buttons
  that perform actions.

### Forms and fields

- Build:
  - A visible label above each field; the placeholder is an example, never
    the label.
  - One column; related fields in a `fieldset` with a `legend`.
  - The right input type and `inputmode` (`email`, `tel`, `numeric`), and
    `autocomplete` values, so phones show the right keyboard and browsers
    fill what they can.
  - Required fields marked in words, or with a mark that is explained.
  - Check on submit and when a field is left, not on every keystroke. The
    error sits under its field, in words, tied to it with
    `aria-describedby`; a long form also lists the errors at the top.
  - Keep what the user typed after an error. Show that the form sent, and
    what happens next.
- Avoid: placeholder-only fields, errors shown only as a red border, a reset
  button, a submit that clears everything on failure.

### Choices: select, radio, checkbox, switch

- Build: radios for one choice from up to about five, visible at once; a
  native `select` for a longer list; a searchable combobox for long lists;
  checkboxes for any number of choices; a switch only for a setting that
  applies immediately. Each has a label that toggles it when clicked, and a
  group has a legend.
- Avoid: a dropdown with two options, a switch inside a form that only
  applies on submit.

### Tables

- Use them for: comparing records on the same fields.
- Build:
  - Columns chosen from the decision the user makes there, the deciding
    field first.
  - Text left-aligned, numbers right-aligned with `font-variant-numeric:
    tabular-nums`, units in the header.
  - A sticky header on long tables; sortable columns marked with
    `aria-sort`.
  - Row actions that exist: visible when there are one or two, in a labelled
    menu when more.
  - An empty state in the table's place, and on a phone either a scroll
    container with a visible edge or a reflow to one card per row.
- Avoid: Name / Status / Date / Actions whatever the data is, a three-dot
  menu on every row with nothing real in it, a table where a list would do.

### Lists

- Build: consistent rows with the most important field first; secondary
  details quieter; an avatar or icon only when it tells rows apart.
- Avoid: dividers and cards and shadows on every row at once.

## Overlays

### Dialogs (modals)

- Use them for: a decision that interrupts, a short form tied to the current
  screen. Content that deserves a URL is a page instead.
- Build:
  - The `dialog` element, or `role="dialog"` with `aria-modal="true"` and a
    title it is labelled by.
  - Focus moves into it when it opens, stays inside, and returns to what
    opened it when it closes.
  - Escape and a visible, labelled close button both close it.
  - One primary action. A destructive confirmation names what will be lost:
    "Delete 3 invoices? This cannot be undone."
- Avoid: a dialog on page load (newsletter, cookie walls beyond what the law
  requires), dialogs opened from dialogs.

### Drawers and sheets

- Use them for: details or a secondary task beside the current view.
- Build: the same focus and closing rules as a dialog; a width that leaves
  the page visible on wide screens; full width on a phone.

### Menus and dropdowns

- Build: a `button` with `aria-expanded` that opens the menu; arrow keys move
  through the items, Enter picks, Escape closes and returns focus; the menu
  stays inside the window.
- Avoid: menus that open on hover only, a menu with one item.

### Tooltips

- Use them for: the name of an icon-only control, a short hint.
- Build: shown on hover and on keyboard focus, dismissed with Escape, text
  only.
- Avoid: information the user needs to finish the task, links or buttons
  inside a tooltip.

## Feedback

### Notifications: toasts, inline alerts, banners

- Build:
  - A toast for a short confirmation of something the user just did,
    dismissed after four to six seconds, with an undo when the action can be
    undone, announced with `role="status"`.
  - An error that needs action stays: inline next to its cause, with
    `role="alert"`, until it is fixed.
  - A banner for a state that affects the whole page (offline, a plan that
    expired), below the header.
  - Text first, colour and icon second.
- Avoid: errors as toasts that vanish, a stack of toasts, "Success!" with
  nothing about what succeeded.

### Badges, tags and chips

- Use them for: real status or real categories, in a few colours with text.
- Avoid: "New", "AI powered", "Beta" with no meaning, a badge on everything.

### Progress and loading

- Build: a skeleton only where it matches the layout that will load; a
  spinner with text saying what is loading; a progress bar with numbers for
  long tasks; buttons show their own loading state.
- Avoid: a full-page spinner for a small update, skeletons shaped like a
  layout the data never fills.

## Wayfinding

### Tabs

- Use them for: switching between views of the same thing (Details,
  Activity, Settings), two to six of them.
- Build: `role="tablist"`, `tab` and `tabpanel`; arrow keys move between
  tabs; the selected tab marked by more than colour; on a phone the tab row
  scrolls sideways or becomes a select.
- Avoid: tabs for steps that must be done in order (use a stepper), tabs
  that navigate to other pages.

### Breadcrumbs

- Use them for: hierarchies more than two levels deep.
- Build: a `nav` with `aria-label="Breadcrumb"`, the current page last and
  not a link.

### Pagination and loading more

- Build: numbered pages when position matters (search results, records);
  a "Show more" button for feeds; infinite scroll only when nothing needs
  the footer, and never without keeping the scroll position on return.

### Search

- Build: a labelled input with a button or Enter to search; results that
  say how many were found for what; a no-results state that suggests what
  to try; the query kept in the URL.
- Avoid: a search box that searches nothing yet.

## Media and data

### Images

- Build: `width` and `height` set, or `aspect-ratio`, so nothing jumps;
  `srcset` and `sizes` for photographs; `loading="lazy"` below the fold;
  alt text that says what the image shows.
- Avoid: stock photographs of people presented as the team or the
  customers, images of text.

### Avatars

- Build: the person's photo when there is one, otherwise their initials on a
  colour derived from the name; a size from the scale; an alt of the
  person's name when it carries information.
- Avoid: generated faces, invented names under them.

### Charts and figures

- Build: a title that states the question the chart answers ("Failed jobs
  per hour, last 24 hours"), labelled axes with units, the source, and a
  table or a sentence with the same information for those who cannot see
  the chart.
- Avoid: decorative charts, a donut for two values, a trend line with no
  data behind it.
