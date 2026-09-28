---
name: ui-page-settings
description: "How to lay out a settings screen without the generated look. Read before building or reworking one."
---

# Settings

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses (header, hero, table, form) has its own
`ui-part-*` skill. Start from this skeleton, then cut and reorder for the
content.

A left column of sections (or tabs on a phone), a single column of fields at
most 640px wide, related settings grouped under a heading, each group with
its own save or saving on change, said once and consistently.
