---
name: ui-part-pagination
description: "How to build pagination and loading more without the generated look. Read before building or reworking one."
---

# Pagination and loading more

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: numbered pages when position matters (search results, records);
  a "Show more" button for feeds; infinite scroll only when nothing needs
  the footer, and never without keeping the scroll position on return.
