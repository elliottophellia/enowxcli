---
name: ui-part-cards
description: "How to build feature lists and cards without the generated look. Read before building or reworking one."
---

# Feature lists and cards

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use cards for: parallel, self-contained items a user compares or picks
  from. A list with a short line each is usually clearer.
- Build:
  - Emphasis follows importance: the main feature can be larger, or shown
    with a screenshot, and the rest listed.
  - A card that is a link is one link: the title's link stretched over the
    card, never a button nested inside a link.
  - Equal heights come from the grid, content aligned to the top.
  - An icon only when it helps recognition, from the product's one set.
- Avoid: three identical cards with an icon in a coloured circle each, a
  card around every paragraph, "Lightning fast / Secure / Scalable".
