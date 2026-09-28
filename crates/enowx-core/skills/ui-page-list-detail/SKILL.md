---
name: ui-page-list-detail
description: "How to lay out list and detail screens without the generated look. Read before building or reworking one."
---

# List and detail

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses (header, hero, table, form) has its own
`ui-part-*` skill. Start from this skeleton, then cut and reorder for the
content.

A list (table or rows) with filters above it; selecting opens the detail on
its own page, or in a side panel 400 to 560px wide when comparing between
items matters. The list keeps its scroll position when the detail closes.
