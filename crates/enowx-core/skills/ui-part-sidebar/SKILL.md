---
name: ui-part-sidebar
description: "How to build an application sidebar and how it changes at each width (full, icon rail, drawer behind a Menu button), with the content anchored beside it. Read before building or reworking one."
---

# Sidebar

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill; the shell it sits in,
with its widths and a skeleton, in `ui-layout` section 5.

- Use it for: an application with more sections than a top bar holds, where
  users move between them all day.
- Build:
  - 224 to 256px wide, full window height, sticky, with its own scroll when
    the list is long. The product name at the top; the daily places next,
    grouped under short headings when there are more than about seven; the
    account and settings at the bottom. At most two levels.
  - Each item an icon and a label on one line, 36 to 40px high; the current
    item marked with a tinted background or a bar and bold text, and
    `aria-current="page"`. A filled, saturated block for the current item
    only when the direction calls for it.
  - The content beside it starts at its edge plus the page padding; it does
    not float centred in the space that is left (`ui-layout` section 2b).
  - 1024 to 1279px: it may collapse to a 64 to 72px rail of icons, each with
    a tooltip and an accessible name, and a control to expand it.
  - Under 1024px it is gone from the page. A sticky top bar (56px) holds a
    labelled "Menu" button (the icon and the word) that opens the same
    navigation as a drawer from the left: a scrim behind it, focus moved in
    and kept there, `Esc` and a close button, closed after a link is chosen,
    focus back on the button.
- Avoid: a sidebar that stays open on a phone or tablet and squeezes the
  content; a sidebar on a marketing site; every item a different coloured
  icon; a user card with an invented name and avatar; nesting three levels;
  the account block pushing the navigation off a short screen.
