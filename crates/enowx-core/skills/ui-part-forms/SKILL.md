---
name: ui-part-forms
description: "How to build forms and fields without the generated look. Read before building or reworking one."
---

# Forms and fields

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build:
  - A visible label above each field; the placeholder is an example, never
    the label.
  - One column; related fields in a `fieldset` with a `legend`.
  - The right input type and `inputmode` (`email`, `tel`, `numeric`), and
    `autocomplete` values, so phones show the right keyboard and browsers
    fill what they can.
  - Required fields marked in words, or with a mark that is explained.
  - Check on submit and when a field is left, not on every keystroke. The
    error sits under its field, in words, tied to it with
    `aria-describedby`; a long form also lists the errors at the top.
  - Keep what the user typed after an error. Show that the form sent, and
    what happens next.
- Avoid: placeholder-only fields, errors shown only as a red border, a reset
  button, a submit that clears everything on failure.
