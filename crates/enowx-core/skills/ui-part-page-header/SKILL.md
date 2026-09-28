---
name: ui-part-page-header
description: "How to build and place the page header inside an application: the title, the one primary action, secondary actions, breadcrumbs, and what happens to them on a phone. Read before building or reworking one."
---

# Page header (inside an app)

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill; where it sits among the
rest, in `ui-layout` section 4.

- Use it for: the title of the current screen and its one primary action.
- Build:
  - One row: the title (the page's `h1`, 20 to 28px) at the left, the
    primary action at the right, both vertically centred on the row. The
    action's right edge lines up with the right edge of the content below.
  - Secondary actions to the left of the primary one, quieter (outline or
    ghost), or in a "More" menu past two. Refresh is not an action: data
    reloads itself after changes.
  - Breadcrumbs above the title when the hierarchy is deeper than one level;
    a count or status beside the title when it helps ("Products · 214").
  - A description under the title only when a first-time user needs it, one
    line; never a sentence that restates the title.
  - No card, border or background of its own: it sits on the page, 16 to
    24px above the first block.
  - On a phone: the title and the primary action stay on one row (the action
    as a button with its word, shortened if needed: "Add"), or the action
    moves into the top bar; secondary actions go into a menu. Nothing wraps
    below or runs off the edge.
- Avoid: a row of five equal buttons; a title that repeats the navigation
  label with nothing added; a subtitle under every title; a "Reload" button;
  a header boxed in its own card.
