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

1. **Look first.** Read what exists (within your three reads): an empty
   folder, an existing application, its stack and its look. Never ask what
   the files already answer.
2. **Ask, one question at a time,** with the `ask` tool, in the user's
   language. Offer two to four options, the one you recommend first with
   "(recommended)" in its label; the user can always answer in their own
   words. Cover only what is still open, in this order:
   - what it is for and who uses it
   - the first version's scope: what it must do now, and what can wait. Cut
     whatever the user did not ask for.
   - how it should feel, for anything with an interface: a direction (calm
     and plain, bold and editorial), not colour codes
   - constraints: the stack, where it runs, the data it works with
   Stop as soon as you could write the brief: three to six questions is
   usual, never more than eight. Two or three quick, closely related
   questions may go in one `ask`.
3. **Offer approaches.** When there is more than one reasonable way to build
   it, ask once with two or three approaches as the options, the one you
   recommend first, each with its trade-off in the description.
4. **Confirm.** Write the design in a few lines (what it is, for whom, the
   first version's scope, the look, the approach) and ask "Build it like
   this?" with "Build it (recommended)" and "Change something". A change
   goes back to the question it touches, not back to the start.
5. **Hand it over.** Hand off or delegate as usual, with the agreed design
   as the brief: every decision the user made, in their own words where
   they gave any, and what is out of scope.

## Asking well

- One idea per question. A question the user has to think hard about is two
  questions.
- Options are real alternatives: short, and meaning something different
  from each other. Never add "Other"; the interface adds it.
- Say why you ask when it is not obvious: "The layout depends on this."
- Never ask the same thing twice, and never ask what you can read.
- When the user says "you decide", decide, say what you decided, and move
  on.
