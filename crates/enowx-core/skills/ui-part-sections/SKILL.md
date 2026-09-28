---
name: ui-part-sections
description: "How to compose the main content between the hero and the footer: each section's point, its composition, its evidence and its density. Read before building or reworking a page's sections."
---

# Sections: the main content

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

A generated page gives itself away in its main content by repeating one
section shape down the page: a label heading ("Selected work", "About",
"Contact"), a line of grey text explaining the section, then a list or a
grid of equal items between hairlines, in one narrow column inside a wide
container. Decide each section on its own.

## For each section, decide

1. **Its point, as a sentence.** The heading says the point ("Tools that run
   on your own machine", "Booking takes two messages"), not the category
   ("Projects", "How it works"). A category heading is right only for a
   section people look for by name: Contact, Pricing, FAQ.
2. **The shape of its content,** which sets the composition:
   - items the reader compares (projects, plans, releases): a list or a
     table with aligned columns;
   - one item that matters most: a feature, larger, with its evidence;
   - an argument or a story: prose at 60 to 70 characters a line, with an
     image or a quote beside it;
   - steps: numbered, only when there really are steps.
3. **Its evidence:** a real screenshot, a real command or code example, a
   real number with its source, a quote with a name. A section with no
   evidence is usually two sentences that belong in another section.

## Use the width

- One column of text in a 1200px container leaves half of a wide screen
  empty, and reads as unfinished. Either use the grid, or narrow the page.
- Using the grid: a label column (3 or 4 of 12) holding the section's
  heading, or each item's name and its meta, beside a content column (8 or 9
  of 12) holding the description or the prose; or the content beside its
  evidence (6 and 6, 7 and 5).
- Narrowing the page: when it is mostly text, set the whole page on a 720
  to 880px container, so the measure is right and nothing looks missing.

## Rhythm

- Vary the compositions: a feature, then a list, then prose beside a quote.
  The same heading, subtitle and list three times over is a template.
- Vary the density: only the lead items (one to three) get the full
  treatment, their description, their evidence (a screenshot when there is
  one) and their details; the rest are compact rows of a name, one sentence
  and a link. Four or more tall blocks built the same way, one after
  another, is a template, and `preview` reports it.
- Space between sections 96 to 128px on wide screens, 64 to 80px on phones;
  within a section, less. A change of background can mark one section that
  changes pace, once or twice on a page, not every other section.

## Hierarchy

- A heading outranks what it contains: a group's heading ("Agents that run
  on your machine") is larger or heavier than the names of the items in it.
  When the items' names are bigger than their group's heading, the page reads
  upside down.
- Three levels are usually enough on one page: the section, the group, the
  item. Style each level one way everywhere.

## Items in a list

- Each item says what it is for someone who has never seen it, in one or two
  plain sentences written for this page, with one concrete detail: what it
  replaces, how it works, who uses it. A one-line description copied from a
  repository ("Agentic Coding Tools") says nothing.
- Metadata only when it helps a decision: the language for developers, a
  date when recency matters, a count only when it is large enough to impress
  a stranger (hundreds, not dozens). Not a meta line under every item by
  habit.
- The item's name is the link, and says where it goes: the repository, the
  live site, a write-up. A "Read more" line repeated under every item is
  noise.
- Items set as splits (text beside an image) alternate sides only when every
  item has its image. An item without one becomes a text row (label column
  and content column); a split with an empty half reads as a missing image.
- A command example fits its column: short lines, a comment on its own line
  rather than aligned at the end, nothing clipped at the edge on a phone.

## Avoid

- A subtitle under each heading that explains the section ("Six of my
  repositories, ordered by stars.").
- An uppercase eyebrow or a section number ("01 /") over every heading.
- Every section a centred heading above a grid of equal cards; a bento grid
  of unrelated things; skill bars, percentages and language-count tables; a
  wall of technology logos.
- Counts as content: followers, stars, repositories and years in a row.
- A narrow column of text in a wide container, with the right half empty.
