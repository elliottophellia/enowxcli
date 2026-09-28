---
name: ui-part-header
description: "How to build the header (top bar) without the generated look, sticky by default. Read before building or reworking one."
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
  - Sticky by default, on a phone too: `position: sticky; top: 0`, with a
    `z-index` above the content and below menus and dialogs, so the
    navigation and the primary action are always one reach away.
    - Its background is solid, from the surface token: content scrolls
      under it, and a transparent bar lays text over text.
    - `scroll-padding-top` on `html`, its height plus 8px, so a link to a
      section does not land under the bar.
    - Nothing around it has `overflow` other than `visible` or `clip`: a
      wrapper with `overflow: hidden`, or `overflow-x: hidden` on both
      `html` and `body`, silently stops it sticking. Fix a horizontal
      overflow at its cause instead.
    - `sticky`, not `fixed`: it keeps its place in the flow, so nothing needs
      a padding-top to clear it.
    - Only the bar with the navigation sticks: an announcement strip above
      it scrolls away, and a header taller than 72px shrinks to its bar once
      the page scrolls.
    - The phone menu opens under the bar and scrolls inside itself:
      `max-height: calc(100dvh - <bar height>)`.
    - Not sticky: a page that fits one screen (sign-in, a short form), and
      an app shell whose content area scrolls on its own, where the bar
      stays put by the layout.
  - A "Skip to content" link as the first thing a keyboard reaches.
  - The current page marked with `aria-current="page"` and more than colour
    (weight or an underline).
  - On a phone the links collapse into a button labelled "Menu", not an
    unlabelled icon.
- Check: the `preview` tool scrolls each long page and reports a top bar
  that scrolls away.
- Avoid: a glass header with a glow, seven links to pages that do not exist,
  "Log in" beside a gradient "Get started" pill, a "Beta" badge next to the
  logo; a bar that scrolls away on a long page, one that sticks with a
  transparent background, section links that land under it.
