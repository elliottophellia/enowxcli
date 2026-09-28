---
name: ui-part-tables
description: "How to build tables without the generated look. Read before building or reworking one."
---

# Tables

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

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
