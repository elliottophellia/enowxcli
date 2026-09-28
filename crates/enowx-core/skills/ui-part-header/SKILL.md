---
name: ui-part-header
description: "How to build the header (top bar) without the generated look, sticky by default. Read before building or reworking one."
---

# Header (top bar)

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: saying whose site or product this is, and reaching the three
  or four places people go most. Not for everything the site has. It is
  read on every screen, so it is the quietest thing on the page that still
  does that job.
- Build:
  - The name as it is written, linking home: the text face at 16 to 18px,
    weight 600, or the real logo when there is one. No full stop,
    underscore, bracket or blinking cursor after it, and no generated
    monogram.
  - Three or four links at most, named with the nouns the page uses
    ("Projects", "Writing", "Contact"), at body size in the text colour. The
    current one is marked by weight or an underline with `aria-current`, not
    by a pill.
  - One action only when the product has one (Book, Download, Sign in), as a
    real button. A link to a profile (GitHub, LinkedIn) is a text link, or an
    icon with a label, not an outlined button beside the navigation.
  - A theme toggle only when both themes are built and checked: one icon
    button whose `aria-label` says what it switches to.
  - The name's left edge and the last link's right edge line up with the
    content's container, so the header and the page share one grid.
  - For a personal site the whole header can be one line of text on one
    baseline: the name, the links, the contact link, with no boxes at all.
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
  logo; a name with a coloured full stop or a cursor; a GitHub button with
  its icon in a bordered box; a search shortcut that searches nothing; a bar
  that scrolls away on a long page, one that sticks with a transparent
  background, section links that land under it.
