---
name: ui-part-breadcrumbs
description: "How to build breadcrumbs without the generated look. Read before building or reworking one."
---

# Breadcrumbs

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use them for: hierarchies more than two levels deep.
- Build: a `nav` with `aria-label="Breadcrumb"`, the current page last and
  not a link.
