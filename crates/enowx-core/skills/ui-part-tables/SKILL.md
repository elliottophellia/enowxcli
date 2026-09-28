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
  - The component library's data table (TanStack Table underneath) rather
    than a hand-built `<table>` once it sorts, filters or pages.
  - Columns chosen from the decision the user makes there, the deciding
    field first, with widths set so they do not jump between rows.
  - Dense: 36 to 44px rows, 14px text, 12 to 16px cell padding, the header
    in a quieter weight and colour, a hairline between rows rather than a
    box around each.
  - The table starts with its header row: no empty band above it, no title
    repeated inside the card that holds it.
  - Text left-aligned, numbers right-aligned with `font-variant-numeric:
    tabular-nums`, units in the header.
  - A sticky header on long tables, set below the top bar (`top` equal to
    its height), with a solid background; the first column sticky too when
    the table scrolls sideways. Sortable columns marked with `aria-sort`.
  - Row actions that exist: visible when there are one or two, as quiet
    ghost or outline buttons, in a labelled menu when more. Never the page's
    filled primary button repeated on every row.
  - An empty state in the table's place, and on a phone either a scroll
    container with a visible edge or a reflow to one compact card per row:
    the identifying field as the heading, the rest as a meta line and a
    status line, without repeating every column's label on every card.
- Avoid: Name / Status / Date / Actions whatever the data is, a three-dot
  menu on every row with nothing real in it, a table where a list would do.
