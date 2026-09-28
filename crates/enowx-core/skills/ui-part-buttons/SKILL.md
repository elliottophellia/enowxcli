---
name: ui-part-buttons
description: "How to build buttons without the generated look. Read before building or reworking one."
---

# Buttons

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build:
  - One primary button per view; secondary and text buttons below it in
    weight.
  - The label is a verb and an object: "Create invoice", "Send reminder".
  - A `button` element (`type="button"` unless it submits), at least 44px
    tall to the touch, padding from the spacing scale.
  - Loading: a spinner inside, the width kept, `aria-busy="true"`, clicks
    ignored. Disabled: say why near it, or leave it enabled and explain on
    click.
  - A destructive action looks it (the danger colour, a confirming step).
- Avoid: an arrow on every button, pill plus gradient plus glow as the
  default look, "Submit", "Click here", two primaries side by side.
