---
name: ui-page-dashboard
description: "How to lay out a dashboard or application home without the generated look. Read before building or reworking one."
---

# Dashboard or application home

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses (header, hero, table, form) has its own
`ui-part-*` skill. Start from this skeleton, then cut and reorder for the
content.

Built around the decision the user makes there, not the default shell.
Page header with the title and the primary action. Then the thing that needs
attention (what is failing, what is due), then the working list or table,
then summaries. Figures only when each one changes what the user does next.

Build it:

- **Start from the question.** Name the two or three decisions a user makes
  on this screen ("what is failing", "who owes money", "what is due today").
  Every block on the page answers one of them; a block that answers none is
  cut.
- **Layout.** Page header (title, date range or scope, primary action). Then
  an attention strip: only items that need action, each with the action next
  to it; empty when nothing does ("Nothing needs attention"). Then the
  working area: the table or list the user acts on, full width, with filters
  above it. Summaries (figures, charts) after, or in a right column of 3 to 4
  of 12 on wide screens.
- **Figures.** At most four, each with its unit, its period and its
  comparison ("12 overdue invoices, 3 more than last week"). A figure with
  no decision behind it is decoration. Right-aligned tabular numbers.
  Deltas only with a real previous period.
- **Charts.** One question per chart, stated in its title, with real data
  and labelled axes. A number or a sentence often answers better; see
  `ui-part-charts`.
- **Density.** Tools are dense: 14px body text, 32 to 40px rows, 16 to 24px
  between blocks. Consistent row heights and aligned columns matter more
  than space.
- **States.** A first-run dashboard has no data: show the one step that
  fills it, not zeroes. Loading keeps the layout (skeletons matching the
  real blocks); errors sit on the block that failed, not the whole page.
- **Navigation.** A sidebar only when there are more than five places to
  go (`ui-part-sidebar`); otherwise a top bar (`ui-part-header`).
- **On a phone.** The attention strip first, then the list as rows, then
  the figures; charts shrink to their headline number with the chart behind
  a tap.

Avoid: the default admin shell (sidebar, four stat cards with invented
numbers and green deltas, a line chart titled "Overview", a recent activity
feed of made-up people, a table of Name/Status/Date/Actions), gradients on
cards, a greeting ("Welcome back, John!") taking the top of the screen.

