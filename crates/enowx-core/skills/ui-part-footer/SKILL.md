---
name: ui-part-footer
description: "How to build the footer without the generated look. Read before building or reworking one."
---

# Footer

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: what people look for at the end. How to reach the owner; the
  address and hours for a local business; the legal pages that exist;
  secondary links.
- Build:
  - For a personal site or a small product, usually one or two lines: the
    contact again (the email itself, as a link), the other profiles that
    exist, and the name with the year. A closing line in the owner's voice
    can end the page better than any grid of links.
  - For a larger site, as many columns as there are real groups of links,
    often one or two.
  - The owner's real name in the copyright line, or a placeholder.
  - On a long page, the back-to-top control (`ui-part-back-to-top`) and the
    footer do not overlap.
- Avoid: "Built with Next.js and Tailwind", a "made with" line with a heart,
  "All rights reserved" boilerplate, a "last synced" timestamp nobody asked
  for, the four-column Product / Company / Resources / Legal template, social
  icons for accounts that do not exist, a newsletter form that sends nothing.
