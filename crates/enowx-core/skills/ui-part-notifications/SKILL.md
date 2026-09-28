---
name: ui-part-notifications
description: "How to build notifications: toasts, inline alerts, banners without the generated look. Read before building or reworking one."
---

# Notifications: toasts, inline alerts, banners

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build:
  - A toast for a short confirmation of something the user just did,
    dismissed after four to six seconds, with an undo when the action can be
    undone, announced with `role="status"`.
  - An error that needs action stays: inline next to its cause, with
    `role="alert"`, until it is fixed.
  - A banner for a state that affects the whole page (offline, a plan that
    expired), below the header.
  - Text first, colour and icon second.
- Avoid: errors as toasts that vanish, a stack of toasts, "Success!" with
  nothing about what succeeded.
