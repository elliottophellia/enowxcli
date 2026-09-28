---
name: ui-page-portfolio
description: "How to lay out a portfolio (a person's work: developer, designer, photographer, writer) without the generated look, including one built from a GitHub account. Read before building or reworking one."
---

# Portfolio

One kind of page. The measures (container, grid, spacing, type) are in
`ui-layout`; each part it uses has its own `ui-part-*` skill. Start from this
skeleton, then cut and reorder for the content.

The work is the page. A portfolio is judged in its first screen by one
question: what does this person make, and is it good? Answer it with the
work, not with a greeting.

## Skeleton

1. Header: the name, three links at most, the one contact link
   (`ui-part-header`).
2. First screen: a claim that says what this person makes, specifically,
   and the work beginning in the same screen (`ui-part-hero`: "claim and
   proof side by side", "statement over the work" or "index first").
3. The work: the strongest three to eight pieces, led by the best. Each with
   its name, what it is for someone who has not seen it (two plain
   sentences), one concrete detail, and its evidence: a screenshot, a
   photograph, a short real example of it in use. Group them by what they do
   when there are many ("Agents", "Automation", "Game tools"), not by
   language or date (`ui-part-sections`).
4. About: three to five sentences in the first person with specifics: where,
   what they work on, what they care about in the work. A real photo if one
   is given.
5. Contact: the email as a link (or a placeholder), and the profiles that
   exist.
6. Footer: one line (`ui-part-footer`).

The header sticks and stays slim so it does not crop the work, and a page of
projects runs long, so it has a back-to-top control
(`ui-part-back-to-top`).

## Built from a GitHub account

- Read each chosen repository's README, not only its one-line description:
  that is where what it does, how it works and its screenshots are. Use the
  README's own images (screenshots, diagrams) as the evidence, taken from
  the repository; when it has none, a short real usage example from the
  README (the command that installs or runs it) in a code block.
- Choose; do not list everything. Forks, empty repositories, dotfiles,
  configuration and abandoned experiments stay out. Four to eight projects,
  grouped by what they do.
- Rewrite each description for this page in plain words; do not paste the
  repository's tagline.
- The lead projects (one to three) show an image when their README has one:
  a screenshot or a diagram, with a caption. The rest are compact rows.
- Stars, followers and language counts are not content. A star count may sit
  in a project's meta when it would impress a stranger (hundreds); a table of
  how many repositories use each language never.
- The chosen projects are written into the page. The repositories left out
  are one link ("All 16 repositories on GitHub"), not a second list of their
  one-line descriptions fetched at load time.
- Repository creation dates are not a career history: there is no timeline
  unless the user gives one.
- Anything about the person beyond GitHub (email, employer, history, photo)
  is a visible placeholder until the user gives it. The placeholder reads as
  one ("[your email]"); how to fill it in goes in the README, never as a note
  on the page.

## The default to avoid

The generated developer portfolio: a dark page, monospace for everything
technical, one amber or green accent, a name with a coloured full stop, a
64px avatar, a claim and a "See the projects" button, "Selected work /
About / Contact" as hairline lists with a star count on every row, a
language table, and a "Built with" footer. Dark with monospace can be right
for a developer, but as a decision a concept asks for (a page that reads
like a well-made manual, with real command output as its images), not as the
starting point.
