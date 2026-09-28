---
name: ui-part-faq
description: "How to build an FAQ without the generated look. Read before building or reworking one."
---

# FAQ

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: questions real users ask, answered briefly.
- Build: `<details>` and `<summary>`, or an accordion with `aria-expanded`;
  one question per item; answers of a few sentences with links onward.
- Avoid: template questions that fit any product ("Is my data secure?"). No
  known questions means no FAQ.
