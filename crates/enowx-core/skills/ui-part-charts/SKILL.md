---
name: ui-part-charts
description: "How to build charts and figures without the generated look. Read before building or reworking one."
---

# Charts and figures

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: a title that states the question the chart answers ("Failed jobs
  per hour, last 24 hours"), labelled axes with units, the source, and a
  table or a sentence with the same information for those who cannot see
  the chart.
- Drawn with the component library's chart kit or an established library
  (Recharts, Chart.js, ECharts), themed with the tokens: one accent for the
  series that matters, neutrals for the rest, gridlines faint or none.
- A split by category is a sorted bar list with values, easier to read than
  a donut; a trend is a line or bars over time with the period in the title.
- Responsive: the chart takes its container's width; on a phone it shows its
  headline number, with the chart below it or behind a tap.
- The chart library loads when the chart is on screen (`import()`), not with
  the page.
- Avoid: decorative charts, a donut for two values, a trend line with no
  data behind it.
