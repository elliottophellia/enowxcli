---
name: ui-part-tabs
description: "How to build tabs without the generated look. Read before building or reworking one."
---

# Tabs

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use them for: switching between views of the same thing (Details,
  Activity, Settings), two to six of them.
- Build: `role="tablist"`, `tab` and `tabpanel`; arrow keys move between
  tabs; the selected tab marked by more than colour; on a phone the tab row
  scrolls sideways or becomes a select.
- Avoid: tabs for steps that must be done in order (use a stepper), tabs
  that navigate to other pages.
