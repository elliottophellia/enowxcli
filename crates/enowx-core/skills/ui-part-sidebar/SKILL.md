---
name: ui-part-sidebar
description: "How to build an application sidebar without the generated look. Read before building or reworking one."
---

# Sidebar

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

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
