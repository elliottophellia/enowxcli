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
- Modular by feature: a folder per feature or area (`billing/`, `auth/`,
  `projects/`) holding its components, logic, types and tests together; the
  truly shared pieces in one place (`components/ui`, `lib`, `utils` only for
  what is generic). No file past a few hundred lines that could be split
  along a real seam; no page that holds its own copies of shared parts.
- Layers stay apart: interface, state, data access and domain logic in their
  own modules, so each can change and be tested without the others.

### File length

- Aim for files under about 300 lines, and components under about 200. Past
  roughly 400, a file you are writing is split along a real seam: a
  sub-component, a hook or composable, the types, the helpers, the constants,
  one module per route or command.
- Longer is fine when splitting would scatter one idea: generated code,
  migrations, a data table or fixture, one cohesive algorithm, a test file
  with many cases, a config. Say so in a comment at the top only if a reader
  would otherwise wonder.
- **New code** you write is modular from the start.
- **Existing files** are not split, moved or restructured on your own
  initiative, however long: the owner may have reasons, and a restructure
  mixed into a fix makes both hard to review. When a file you touch is past
  the limit, ask first with `ask` (for example "Split `Dashboard.tsx` (820
  lines) into `StatCards`, `LoansTable` and `TrendChart` (recommended)",
  "Only extract what I am changing", "Leave the structure as it is"), and do
  only what the answer allows. Without `ask` (a delegated task), leave the
  structure, make your change, and propose the split in your report.

## 3. Names

- Names say what a thing is or does, in the domain's words:
  `unpaidInvoices`, not `data2`; `sendReminder`, not `handleStuff`.
- Booleans read as questions (`isOpen`, `hasAccess`), functions as verbs. No
  abbreviations the codebase does not already use.
- Match the codebase's casing and vocabulary, even where you would choose
  differently.
- Names are English: files, folders, components, functions, variables,
  types, routes, database tables and columns, i18n keys, commit messages,
  unless the user asks for another language or the codebase already uses
  one. Text the user reads is in their language, through i18n (the `i18n`
  skill); the code around it stays English.
- File names follow the ecosystem: `PascalCase` components in React and Vue
  (`InvoiceTable.tsx`), `kebab-case` files on the web elsewhere
  (`invoice-table.ts`), `snake_case` in Rust and Python, one component per
  file, named as the thing it exports.

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

## 6a. Data at scale

- Lists are paginated in the query (a cursor or an offset with a limit, on
  an indexed column), never by loading a table and slicing it.
- Counts, sums and series are computed in the database with a grouped
  query; heavy ones are cached (in memory with a short expiry, a summary
  table, a materialised view) and invalidated when their data changes.
- No query in a loop (N+1): load related rows in one query or a join.
- Cache what is read often and changes rarely, at the level it is cheapest:
  HTTP headers (`Cache-Control`, `ETag`) for responses, the client's query
  cache for screens, the server for expensive computations. Every cache has
  a rule for when it is stale.
- Measure before optimising further: the slow query, the large bundle, the
  render that repeats.

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
