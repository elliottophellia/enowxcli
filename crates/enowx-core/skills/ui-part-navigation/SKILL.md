---
name: ui-part-navigation
description: "How to build navigation: top links, dropdowns, the phone menu, a bottom tab bar without the generated look. Read before building or reworking one."
---

# Navigation

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: three to seven destinations, named with the nouns users
  already use ("Invoices", "Clients", not "Solutions").
- Build:
  - Order by how often each place is used, not by the org chart.
  - On a website it lives in the header, which sticks (`ui-part-header`),
    so it is in reach anywhere on the page.
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
