---
name: ui-part-tables
description: "How to build and place tables: one surface with its toolbar and footer, columns and their alignment, statuses and row actions, and how a table fits a narrow screen. Read before building or reworking one."
---

# Tables

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill; where a table and its
toolbar sit, in `ui-layout` section 4.

- Use them for: comparing records on the same fields.
- Build:
  - The component library's data table (TanStack Table underneath) rather
    than a hand-built `<table>` once it sorts, filters or pages.
  - One surface: the toolbar (search, filters, view options) as its top
    row, then the header row, the rows, and a footer with the count at the
    left and pagination at the right. Not a filter card, a gap and a table
    card; no title repeated inside the surface.
  - Columns chosen from the decision the user makes there, the identifying
    field first, then the deciding ones (status, stock, amount, due date),
    then context. Widths set so they do not jump between rows or pages.
  - Dense: 40 to 48px rows, 14px text, 12 to 16px cell padding, the header
    in a quieter weight and colour, a hairline between rows rather than a
    box around each.
  - Alignment: text left, numbers and money right with
    `font-variant-numeric: tabular-nums`, units in the header, and each
    header aligned the same way as its cells. Short values (codes, prices,
    dates, badges) never wrap (`white-space: nowrap`).
  - A status says the exception, not the norm: no badge when a product is
    simply active; "Inactive", "Low stock · 3 left", "Overdue · 2 days" where
    they apply, with the text as well as the colour.
  - The cells show values, not controls: no steppers, inputs or selects in
    every row. A quick change (stock, status) opens a small popover or the
    record's panel; inline editing only in a spreadsheet-like tool, one cell
    at a time, when the brief asks for it.
  - Row actions that exist: one or two visible as quiet ghost buttons or
    links, the rest in a labelled menu at the row's end, and the row (or its
    name) opening the record. Never the page's filled primary button
    repeated on every row.
  - A sticky header on long tables, set below the top bar (`top` equal to
    its height), with a solid background; the first column sticky when the
    table scrolls sideways. Sortable columns marked with `aria-sort`.
  - An empty state in the table's place, with the action that fills it.
- On narrow screens:
  - Columns hide by priority as the width drops (`hidden md:table-cell` on
    the context columns first), keeping the identifying and deciding ones.
  - Below about 640px, either a scroll container with a visible edge and the
    first column sticky, or one compact card per record: the identifying
    field as the heading, a meta line, the status, and the daily action on
    the status line, without every column's label repeated on every card.
  - Never cut off: the table's container, not the page, scrolls sideways.
- Avoid: Name / Status / Date / Actions whatever the data is; a green
  "Active" badge on every row; steppers or inputs in every row; two buttons
  on every row where a menu would do; a three-dot menu with nothing real in
  it; product codes breaking over two lines; a table where a list would do.
