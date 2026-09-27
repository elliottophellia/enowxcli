---
name: code
description: "Writing or changing code so it reads like the codebase and like an engineer wrote it: structure, names, reuse, types, errors, dependencies, comments. Read before a new module or component, or a change of more than a few lines."
---

# Code that belongs to its codebase

Generated code gives itself away: a comment above every line, a banner around
every section, a helper for something done once, `any` where a type was
needed, a new dependency for a ten-line job, errors swallowed so the demo
runs. This is how to write code the next engineer can own.

## 1. Read before you write

- Find the conventions: formatter and linter settings, naming, folder layout,
  how similar things are already done. Open two or three neighbours of the
  file you are about to create, and match them.
- Search before adding: the helper, hook, component or type you need may
  already exist. Extend it rather than writing a second one.
- Stay on the project's stack and versions. Do not bring in a second way of
  doing what the codebase already does one way: a second HTTP client, state
  library, styling method or date library.

## 2. Structure

- One responsibility per function, module and component. A name that needs
  "and" is two things.
- Extract when a pattern repeats (by the third time) or names a concept in
  the domain. Do not build a general abstraction for a single use: three
  similar lines cost less than the wrong abstraction.
- Keep side effects at the edges: network, storage, time and randomness at
  the boundary; pure logic inside, where it can be tested.
- Early returns over nested conditions. Small files that each hold one thing,
  over one file that holds everything.

## 3. Names

- Names say what a thing is or does, in the domain's words:
  `unpaidInvoices`, not `data2`; `sendReminder`, not `handleStuff`.
- Booleans read as questions (`isOpen`, `hasAccess`), functions as verbs. No
  abbreviations the codebase does not already use.
- Match the codebase's casing and vocabulary, even where you would choose
  differently.

## 4. Types and data

- Type the boundaries: props, API responses, function parameters. In
  TypeScript no `any`; `unknown` and a check where the shape really is
  unknown.
- Validate input where it enters (forms, requests, files) and trust it after.
- Model states explicitly (`loading | error | ready`), not as booleans that
  can contradict each other.
- Derive values rather than storing copies that go stale.

## 5. Errors

- Handle an error where something useful can be done about it: retry, fall
  back, or tell the user what happened and what to do.
- Never swallow an error to make something appear to work: no empty
  `catch`, no `catch` that logs and carries on as if nothing happened.
- Messages say what failed and why, with the value that caused it, and never
  a secret.

## 6. Dependencies

- Prefer the platform and what is already installed. Add a package only when
  it does substantial work, is maintained, and is small for what it gives;
  say why in your report.
- Import only what you use.

## 7. Frontend

- Components are functions of their props and state. In React: stable ids as
  keys for lists that change, never array indexes; state in the lowest
  component that needs it; effects only to synchronise with something
  outside React, never to compute a value; memoise after measuring, not by
  habit.
- CSS: tokens (custom properties, or the project's theme) for colour,
  spacing, radius and type; the project's method for scoping classes;
  mobile-first media queries; no `!important`; no inline styles except values
  computed at run time; no fixed heights on anything holding text.
- Tailwind: the theme's scale, not arbitrary values; a class group that
  repeats becomes a component, not a copy.
- Static pages: semantic HTML first, CSS second, JavaScript only for
  behaviour, and the page still reads without it.

## 8. Comments

- Comment why, not what: the constraint, the business rule, the edge case,
  the workaround and what it works around, the security or performance
  trade-off.
- No comment that restates the code, narrates the steps ("Step 1: validate
  input"), labels a region ("Main logic"), decorates (banners, capitalised
  headers, emoji), marks the end of a block, or repeats a signature
  (`@param id the id`). A vague TODO ("improve this") is noise; a TODO names
  the task.
- Match the codebase's comment density. When a block needs a comment to be
  followed, try a better name or a smaller function first.

## 9. Hygiene

- No dead code, commented-out code, debugging output (`console.log`,
  `print`), or unused imports and variables.
- Run the project's formatter and linter on what you touched.
- Never change files by running a script that string-patches them; write the
  change in the source where it belongs.
- Tests follow the project's framework and style, and test behaviour through
  the public surface.

## 10. Before you call it done

- It builds, the linter is clean for what you touched, and the tests pass.
- Read your diff as a reviewer would: every line needed, well named, nothing
  left over.
