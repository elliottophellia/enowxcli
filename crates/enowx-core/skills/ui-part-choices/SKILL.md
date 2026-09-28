---
name: ui-part-choices
description: "How to build choices: select, radio, checkbox, switch without the generated look. Read before building or reworking one."
---

# Choices: select, radio, checkbox, switch

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: radios for one choice from up to about five, visible at once; a
  native `select` for a longer list; a searchable combobox for long lists;
  checkboxes for any number of choices; a switch only for a setting that
  applies immediately. Each has a label that toggles it when clicked, and a
  group has a legend.
- Avoid: a dropdown with two options, a switch inside a form that only
  applies on submit.
