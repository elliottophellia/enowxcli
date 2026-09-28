---
name: ui-part-dialogs
description: "How to build dialogs (modals) without the generated look. Read before building or reworking one."
---

# Dialogs (modals)

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use them for: a decision that interrupts, a short form tied to the current
  screen. Content that deserves a URL is a page instead.
- Build:
  - The `dialog` element, or `role="dialog"` with `aria-modal="true"` and a
    title it is labelled by.
  - Focus moves into it when it opens, stays inside, and returns to what
    opened it when it closes.
  - Escape and a visible, labelled close button both close it.
  - One primary action. A destructive confirmation names what will be lost:
    "Delete 3 invoices? This cannot be undone."
- Avoid: a dialog on page load (newsletter, cookie walls beyond what the law
  requires), dialogs opened from dialogs.
