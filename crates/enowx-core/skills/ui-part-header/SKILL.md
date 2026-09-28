---
name: ui-part-header
description: "How to build the header (top bar) without the generated look. Read before building or reworking one."
---

# Header (top bar)

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

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
