---
name: brainstorming
description: "Turning a new project, a new feature or a redesign into a design agreed with the user before anyone builds it. Read before routing such a request; not for fixes, small changes or questions."
---

# Brainstorming before building

A new project or feature built from a one-line request is built on guesses:
who it is for, what it must do first, how it should look, what it must not
do. Each wrong guess is paid for with a rebuild. A few questions first cost
the user a minute.

## When

Brainstorm when the request starts something whose shape is open:

- a new project or application from nothing ("build me an app for…",
  "scaffold a…")
- a new feature, or a new page, with real choices in it
- a redesign or a rewrite
- anything two reasonable specialists would build differently

Do not brainstorm:

- a bug, an error or a failing test: route it
- a small change whose result is clear ("make the button green")
- a question about the code: answer it or route it
- work the user already specified in detail, or when they say to just
  build it
- a follow-up on work already agreed in this conversation

When it is unclear, ask one question: whether the user wants to settle the
details, or leave them to you.

## How

1. **Look first.** Read what exists (within your five looks): an empty
   folder, an existing application, its stack and its look. Never ask what
   the files already answer.
2. **Ask everything open in one `ask`,** as a list of questions, in the
   user's language. The user moves between them with next and previous and
   sends them together; asking them one call after another makes the user
   answer, wait, and answer again. Give each question a short `header` and
   two to four options, the one you recommend first with "(recommended)"
   in its label; the user can always answer in their own words. Cover only
   what is still open, in this order:
   - what it is for and who uses it
   - the first version's scope: what it must do now, and what can wait. Cut
     whatever the user did not ask for.
   - how it should feel, for anything with an interface: two or three
     concepts drawn from the subject itself, each in a few words (for a
     developer who builds command-line tools: "a well-made manual, with real
     commands and their output", "a catalogue of tools by what they do", "a
     lab notebook of experiments"), not colour codes. Never offer the
     category's default look as an option ("dark developer / mono", "modern
     and clean", "minimalist"): it is what the page becomes without a
     direction (the `ui` skill names the defaults)
   - the theme, for anything with an interface, always asked when a new
     project is scaffolded: light, dark, or both with a toggle. Recommend
     what the concept calls for (a manual reads on paper, a night
     photographer's work on black); "developers like dark" is the category's
     default, not a reason. When "both" is chosen, both themes are built and
     checked, not one with the other left broken
   - how much motion, for a page people read (a launch, a product site, a
     portfolio): feedback only, entrances and transitions, or choreography
     with animated drawings and a demo (the `motion` skill's dial). Recommend
     from the product: a tool used all day moves least, a launch page may
     move most, and every level stays calm
   - for anything with an interface and no existing component library: the
     component library and icon set, as options for the chosen stack with
     the recommendation first (for React with Tailwind: "shadcn/ui with
     Lucide (recommended)", "Mantine with Tabler", "Radix Themes"); the `ui`
     skill lists the choices per stack
   - constraints: the stack, where it runs, the data it works with. How
     the data is fetched, cached or deployed is the specialist's to decide;
     ask about it only when the user raised it
   Ask only what you could not write the brief without: three to six
   questions is usual, never more than eight.
   For a developer's portfolio, the look and the theme might be asked like
   this, each option a concept from the work, the recommended one first:

   ```json
   {"header": "Look", "question": "Which idea should the page be built on?",
    "options": [
     {"label": "A field manual (recommended)", "description": "Each tool as an entry: what it does, the command that runs it and its real output. Light paper, serif headings."},
     {"label": "A catalogue by purpose", "description": "Agents, memory, automation: grouped like a product catalogue, one screenshot per tool."},
     {"label": "A lab notebook", "description": "Dated entries on what was built and why, the repositories as the evidence."}]}
   {"header": "Theme", "question": "Light or dark?",
    "options": [
     {"label": "Light (recommended)", "description": "A manual reads on paper."},
     {"label": "Dark"},
     {"label": "Both, with a toggle"}]}
   ```

   The options come from this person's work and are written fresh each
   time; copy the shape, not the words.
3. **Approach and confirmation, together.** Write the design in a few lines
   (what it is, for whom, the first version's scope, the look), then one
   more `ask`: when there is more than one reasonable way to build it, a
   question with two or three approaches as options (recommended first,
   each with its trade-off), and a last question "Build it like this?" with
   "Build it (recommended)" and "Change something". A change goes back only
   to what it touches, again in one `ask`.
4. **At most two rounds** before building: the questions, then the
   approach and confirmation. When the answers leave nothing open and the
   approach is obvious, skip the second round and say what you will build.
5. **Hand it over.** Hand off or delegate as usual, with the agreed design
   as the brief: every decision the user made, in their own words where
   they gave any, and what is out of scope. Only those: the layout, the
   sections and what goes in them are the specialist's, from its skills. For anything with an interface,
   say that the direction and theme go into `DESIGN.md`, so every later
   change keeps to them.

## Asking well

- One idea per question. A question the user has to think hard about is two
  questions.
- Options are real alternatives: short, and meaning something different
  from each other. Never add "Other"; the interface adds it.
- The option you recommend is the one a good designer would pick, not the
  one that includes the most: a portfolio shows the strongest few pieces
  (four to eight), not every repository; a landing page leads with one
  action; a first version keeps the scope small.
- Say why you ask when it is not obvious: "The layout depends on this."
- Never ask the same thing twice, and never ask what you can read.
- When the user says "you decide", decide, say what you decided, and move
  on.
