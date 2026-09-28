---
name: ui-page-portfolio
description: "How to lay out a portfolio without the generated look. Read before building or reworking one."
---

# Portfolio

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses (header, hero, table, form) has its own
`ui-part-*` skill. Start from this skeleton, then cut and reorder for the
content.

The work is the page. One line saying who and what, then the projects:
large images, each with the problem, the role and the outcome in two or
three lines. A project list is not a grid of equal cards: lead with the
strongest, vary the sizes. About and contact after.

The header sticks and stays slim, so it does not crop the work; a page of
projects runs long, so it has a back-to-top control
(`ui-part-back-to-top`).
