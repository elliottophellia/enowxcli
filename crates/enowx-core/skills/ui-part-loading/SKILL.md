---
name: ui-part-loading
description: "How to build progress and loading states without the generated look. Read before building or reworking one."
---

# Progress and loading

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: a skeleton only where it matches the layout that will load; a
  spinner with text saying what is loading; a progress bar with numbers for
  long tasks; buttons show their own loading state.
- Avoid: a full-page spinner for a small update, skeletons shaped like a
  layout the data never fills.
