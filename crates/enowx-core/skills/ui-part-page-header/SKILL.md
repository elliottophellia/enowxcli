---
name: ui-part-page-header
description: "How to build the page header inside an application without the generated look. Read before building or reworking one."
---

# Page header (inside an app)

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: the title of the current screen and its one primary action.
- Build: the title as the page's `h1`, a short description only if it helps
  a first-time user, the primary action on the right, secondary actions
  quieter or in a menu, breadcrumbs above when the hierarchy is deep.
- Avoid: a row of five equal buttons, a title that repeats the navigation
  label with nothing added.
