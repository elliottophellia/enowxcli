---
name: ui-part-hero
description: "How to build the first screen (the hero) so it says something only this product or person could say, with the proof in view. Read before building or reworking one."
---

# Hero

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

The first screen is where a generated page gives itself away: a name or a
slogan, a line of grey text, one button that scrolls down, and empty space
where the proof should be. The reader decides in that screen whether to go
on, so it carries the most specific thing you have, not the most general.

## Decide before building

1. **The claim.** One sentence only this product or person can say. Test it:
   put a competitor's name in it. If it still holds, it is not specific yet.
   "I build agent infrastructure and local-first software" fits a thousand
   developers; "Everything I build runs on your own machine: agents, a coding
   CLI, crawlers, with no one else's server in between" fits one. Write it
   from the facts you have, never from invented ones.
2. **The proof.** What makes the claim believable, in the same screen: the
   work itself (a real screenshot, a real photograph, the first two or three
   projects), a real command and its real output, a real quote with a name.
   A claim whose proof is one scroll away reads as a claim.
3. **The next step,** only if there is one the reader takes here: book,
   install, download, email. "See my work" or "Learn more" is not a next step
   when the work is right below; leave the button out.

## Compositions

Choose by what the proof is. Do not reach for the first one by habit.

- **Claim and proof side by side** (wide screens): the claim, one line of
  support and the action in 6 or 7 of 12 columns; the proof (a screenshot, a
  photograph, the first projects as a short list) in 5 or 6, aligned to the
  top of the headline, not floating in the middle. On a phone: the claim,
  then the proof.
- **Statement over the work**: the claim large across 9 or 10 columns, set
  left, and the work beginning immediately under it, inside the first
  screen. The hero is the top of the content, not a banner above it.
- **Index first** (a portfolio of many small tools, documentation, a
  changelog): the name and one line as a compact block, then the list of
  work starts at once and is itself the focal point.
- **Product in use** (software): the interface or its real output large, the
  claim beside or above it in one or two lines.

## Type and space

- The claim is the largest type on the page: 39 to 61px on wide screens
  with `clamp()`, 31 to 39px on phones, line height 1.05 to 1.15, at most
  about 20 words a line. Set it in the display face the direction chose.
- The supporting line is body size or one step up, in the text colour or
  slightly muted, never faded to a grey that fails contrast.
- On a laptop (1440 by 900) the first screen shows the start of the content
  below the hero. The hero is as tall as its content, usually 320 to 560px;
  never `100vh`.

## Names and people

- A person's name belongs in the header, or as a byline under the claim. It
  is rarely the best headline: it says nothing about the work.
- A portrait only when a real photo is given, and then with some size (160
  to 320px) as part of the composition. A 64px avatar in a rounded square
  beside a name is decoration.
- No full stop, underscore, bracket or blinking cursor after a name used as a
  logo or a headline ("Enow Dev.", "enowdev_"): it is the default flourish,
  not an identity.

## Avoid

- "Hi, I'm X" with a waving hand, "Welcome to…", "Unlock the power of…", a job
  title as the headline ("Full-stack developer").
- A single button whose only job is to scroll to the next section.
- Half of the first screen empty on a wide window.
- A row of numbers under the claim (followers, stars, years): the work
  proves more than its counts.
- Gradient text, blobs and 3D shapes, a floating "New" pill, a "trusted by"
  logo row, a fake terminal window, a typewriter effect, a pulsing
  "available for work" dot.
