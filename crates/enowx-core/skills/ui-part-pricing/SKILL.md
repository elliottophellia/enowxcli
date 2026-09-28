---
name: ui-part-pricing
description: "How to build a pricing section without the generated look. Read before building or reworking one."
---

# Pricing

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: letting a user choose a plan and know what they pay.
- Build:
  - As many plans as are real. Features in the same order in every plan so
    they can be compared.
  - The price with its currency and period, and what the button does next.
  - One plan highlighted only when there is a reason to recommend it, with
    the reason in words.
- Avoid: three columns by default, "Most popular" on the middle plan with no
  basis, crossed-out prices that were never charged.
