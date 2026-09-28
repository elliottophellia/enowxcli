---
name: ui-page-docs
description: "How to lay out documentation or an article without the generated look. Read before building or reworking one."
---

# Documentation or article

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses (header, hero, table, form) has its own
`ui-part-*` skill. Start from this skeleton, then cut and reorder for the
content.

Three columns on wide screens: navigation 240 to 280px on the left, the text
at 65ch in the middle, the "on this page" list on the right (hidden below
1200px). Headings with anchors. Code blocks full width of the text column.

The header sticks, and both side columns stick under it (`position: sticky`
with `top` set to the header's height), each scrolling on its own when it is
taller than the screen; `scroll-padding-top` keeps a heading reached by its
anchor clear of the header. A long article has a back-to-top control
(`ui-part-back-to-top`).
