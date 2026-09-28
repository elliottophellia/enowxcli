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
- Per block, not per page: each part of a screen loads its own data and
  shows its own skeleton, so the fast parts appear first and one slow query
  blanks nothing else. The frame (navigation, headers) renders at once.
- Refreshing data stays on screen while the new copy loads (stale while
  revalidate); a small spinner in the block's header says it is updating.
- A skeleton appears only after about 200ms, so fast loads do not flash.
- Avoid: a full-page spinner for a small update, skeletons shaped like a
  layout the data never fills, content that jumps when it arrives.
