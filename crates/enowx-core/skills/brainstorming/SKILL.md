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
   - how it should feel, for anything with an interface: a direction (calm
     and plain, bold and editorial), not colour codes
   - the theme, for anything with an interface, always asked when a new
     project is scaffolded: light, dark, or both with a toggle. Recommend
     from the product and its users (a developer tool dark, a clinic's
     booking page light), and when "both" is chosen, both themes are built
     and checked, not one with the other left broken
   - constraints: the stack, where it runs, the data it works with
   Ask only what you could not write the brief without: three to six
   questions is usual, never more than eight.
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
   they gave any, and what is out of scope. For anything with an interface,
   say that the direction and theme go into `DESIGN.md`, so every later
   change keeps to them.

## Asking well

- One idea per question. A question the user has to think hard about is two
  questions.
- Options are real alternatives: short, and meaning something different
  from each other. Never add "Other"; the interface adds it.
- Say why you ask when it is not obvious: "The layout depends on this."
- Never ask the same thing twice, and never ask what you can read.
- When the user says "you decide", decide, say what you decided, and move
  on.
