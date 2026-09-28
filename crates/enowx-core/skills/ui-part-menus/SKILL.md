---
name: ui-part-menus
description: "How to build menus and dropdowns without the generated look. Read before building or reworking one."
---

# Menus and dropdowns

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: a `button` with `aria-expanded` that opens the menu; arrow keys move
  through the items, Enter picks, Escape closes and returns focus; the menu
  stays inside the window.
- Avoid: menus that open on hover only, a menu with one item.
