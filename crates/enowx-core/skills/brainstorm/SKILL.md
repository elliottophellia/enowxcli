---
name: brainstorm
description: "Turning a new project, a new feature or a redesign into a design agreed with the user, then into the plan documents the user chose (PRD, DESIGN, ARCHITECTURE, ERD, API, PLAN), before anyone builds it. Asks which documents to write and skips the rest. Read before routing such a request; not for fixes, small changes or questions. Its parts hold each document's template."
---

# Brainstorm before building

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

Two rules hold for every brainstorm, whatever the work is:

- **Ask only what is the user's to decide; decide how it is built yourself.**
  The user owns what it is for, its scope, how it looks and feels, and its
  content. You own how it is made: the tool, the library, the technique, the
  file format, how data is fetched or stored. Never turn an implementation
  choice into a question, and never ask the user to do a step a tool can do
  (record a sound, draw an asset, export a file by hand). If a build choice
  truly changes what the user will pay, run on or live with, state your
  decision and the trade-off in one line, do not make them pick.
- **Read the one relevant skill before you offer any option.** The skills
  list in your prompt is an index: names and one-line descriptions only. Find
  the single skill whose description matches the work and `skill_read` that
  one (not the others, never all of them), then draw the options and the
  recommendation from it, so they are real, not invented. With no skill for
  it, options may come from your own knowledge, said plainly as that. Never
  list tools or approaches you have not checked are right for this task.

For example, a video or motion graphic as a file (an explainer, an animated
logo, a social clip): its content is worth a short brainstorm (what it shows,
its length, its visual style), but the tool is not a question. Read
`motion-video`; it is rendered from code with the sound synthesised, so you
never ask "which tool", never offer After Effects, Remotion, Manim or a
webview recording, and never ask the user to record the sound.

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
   - the checks, for work large enough to delegate in waves, `"multiple":
     true`: "Tests (recommended)" (the `test` agent writes and runs tests
     against what was agreed), "Review (recommended)" (the `review` agent
     reads every change), and "Security review" when there is sign-in,
     payment, upload or anything public. What the user leaves unticked is
     not delegated; ticking none means only the specialists' own checks run,
     and the final answer says the work was not tested or reviewed beyond
     them
   - which plan documents to write, as the last question, `"multiple":
     true`, when the work is large enough to delegate in waves (section
     "The plan documents"): PRD, DESIGN, ARCHITECTURE, ERD, API, PLAN, each
     option saying in a few words what it fixes, the ones this project needs
     ticked as recommended in their labels. Documents the user does not tick
     are skipped; when they tick none, there is no plan and the design goes
     straight into the briefs. Do not ask it for small work, or when the user
     already said to just build it
   Ask only what you could not write the brief without, and only what is the
   user's to decide: three to six questions is usual, never more than ten.
   Never ask which tool, library, framework, technique or file format to use,
   or ask the user to perform a step a tool can do; those are decided from the
   relevant skill or your own judgement, not by the user.
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
5. **Write the documents the user chose,** with `plan_write`, each from
   its part's template (below), in one or two steps. Skip every document
   the user did not choose.
6. **Hand it over.** Hand off or delegate as usual, with the agreed design
   as the brief (and, when there is a plan, the documents each part reads): every decision the user made, in their own words where
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

## The plan documents

A large task goes badly when it goes straight from the user's message to
four delegations, each brief retelling the project in its own words: the
frontend invents `price` while the backend returns `priceMinor`, nobody
wrote down that the user wanted Indonesian copy, and when a part fails
nothing says what "done" meant. A few short documents, written once from
what the user decided and cited in every brief, fix that. Which ones is the
user's choice, asked in step 2:

| Document | Part | Written to | Fixes |
|---|---|---|---|
| PRD | `brainstorm-prd` | `docs/plan/PRD.md` | What is built, for whom, what is in and out, how each piece is accepted |
| DESIGN | `brainstorm-design` | `DESIGN.md` | The look and the screens: direction, audience, pages, flows, states |
| ARCHITECTURE | `brainstorm-architecture` | `docs/plan/ARCHITECTURE.md` | Stack, folders and who owns them, data flow, decisions |
| ERD | `brainstorm-erd` | `docs/plan/ERD.md` | Entities, fields, relations, keys, indexes |
| API | `brainstorm-api` | `docs/plan/API.md` | Routes, shapes, errors: the contract parts share |
| PLAN | `brainstorm-plan` | `docs/plan/PLAN.md` | Waves, parts, owners, files, done-when, status |

`plan_write` writes only these files; the specialists read them with
`read`. Read a document's part before writing it.

### What to recommend

| The request | Documents |
|---|---|
| A question, a fix, one page, one endpoint, a follow-up | None: hand off or delegate as usual |
| A feature in one area of an existing project | None, or `PLAN.md` alone when it needs more than one wave |
| A feature across areas (a screen, its API, its table) | `PRD.md` (short), `API.md` for the new routes, `ERD.md` for the new tables, `PLAN.md` |
| A new product or app | All that apply: `PRD`, `DESIGN` when it has an interface, `ARCHITECTURE`, `ERD` when it stores data, `API` when a client talks to a server, `PLAN` |
| The user asks for a plan, a spec or a PRD | The documents they asked for, then stop and show them |

These are the recommendations ticked in the question; the user's choice wins.

For a new shop with a catalogue, a cart and an API, the question might be:

```json
{"header": "Plan", "question": "Which plan documents should I write before building?", "multiple": true,
 "options": [
  {"label": "PRD (recommended)", "description": "Scope and how each feature is accepted"},
  {"label": "DESIGN (recommended)", "description": "Look, screens, flows and their states"},
  {"label": "ERD (recommended)", "description": "Products, carts and orders, and how they relate"},
  {"label": "API (recommended)", "description": "The routes the pages and the server agree on"},
  {"label": "ARCHITECTURE", "description": "Stack and folders; the defaults are fine without it"},
  {"label": "PLAN (recommended)", "description": "Who builds what, in which wave, and when it is done"}]}
```

Ticking none is an answer: build from the agreed design with no documents.

- A static site with no data needs no ERD or API; a CLI needs no DESIGN.
  Write only documents that will be read.
- Size to the work: a weekend project's PRD is one screen, not twelve
  sections of boilerplate. Sections with nothing to say are left out, not
  filled with "N/A".
- In an existing project, read what is there first (`README`, existing
  `DESIGN.md`, `docs/`, the schema, the routes) and plan the change against
  it: the documents describe the addition and what it touches, not the
  whole system again.

### The order

1. **The design agreed in the questions above**: audience, scope, look,
   stack when it matters. The documents record decisions; they are not
   where decisions are invented.
2. `PRD.md`: the scope and acceptance, in the user's terms.
3. `DESIGN.md` for anything with an interface.
4. `ARCHITECTURE.md`: stack and layout, from the user's choice, the
   existing project, or the plain default for the kind of project (say
   which).
5. `ERD.md`, then `API.md`: data first, so the routes return what is stored.
6. `PLAN.md` last: it splits the work the others describe.

Write them in one or two steps (several `plan_write` calls together), not
one per turn. Then tell the user in a few lines what the plan says and
start the first wave, unless they asked to review it first or the plan
settled something they did not decide, in which case `ask` once with the
open points as options.

### What goes in, and what never does

- **Only what was decided or is plainly implied.** The user's decisions in
  their words; the project's existing facts; defaults named as defaults
  ("Postgres: the project has no database yet; the usual choice for this
  stack"). Never invented requirements, users, metrics, prices or
  testimonials. Content the user has not given is a placeholder, listed as
  one.
- **Decisions, not essays.** Tables and lists, one fact a line. A
  specialist reads a document in seconds or skips it.
- **Stable identifiers.** Requirements numbered (`FR-3`), entities and
  routes named once and used the same way everywhere; a brief says "build
  FR-1 to FR-4" and everyone knows what that is.
- **Layout and code are not planned.** Which sections a page has, how a
  component is built, which library draws a chart: the specialist's
  skills decide. The plan fixes what parts must agree on: names, shapes,
  routes, files, and what "done" means.

### How the documents drive delegation

- Every brief names the documents and sections its part needs:
  `Read: docs/plan/PRD.md (FR-1 to FR-4), docs/plan/API.md (Products),
  DESIGN.md`. It does not paste them: the specialist reads them.
- The contract in a brief (`orchestration`, section 4) is copied from
  `API.md` and `ERD.md`, word for word, so parts running together agree.
- `PLAN.md` holds the waves; `todo` mirrors them, one item per part.
- The `done when` of each part is the acceptance in `PRD.md` plus the
  checks in `PLAN.md`.

```
Goal: the catalogue page, as the user asked ("halaman katalog yang bisa difilter")
Read: docs/plan/PRD.md (FR-1, FR-2), docs/plan/API.md (GET /api/products), DESIGN.md
Your part: src/app/catalogue/**
Beside you: be owns src/server/** (the API); db owns db/**
Contract: GET /api/products?cursor=&limit=&category= → { items: Product[], nextCursor }
  Product and Category from src/shared/types.ts (written in wave 1; do not edit)
Done when: FR-1 and FR-2 acceptance holds; builds; preview clean at 360, 768, 1440px
```

### Keeping them true

- After each wave, read the reports and update `PLAN.md`: each part's
  status (done, partial, failed), what changed from the plan, and what the
  next wave needs to know.
- When a report shows the plan was wrong (a route had to change, a field
  was missing), fix the document first, then send the change to the parts
  it touches. The document is where the next brief reads it from.
- A decision the user makes later goes into the document it belongs to,
  and the `Decisions` list at the end of `PRD.md`, with the date.
- Never let the documents describe work that was not done: "planned",
  "built" and "verified" stay separate.

### Check the plan

- Every document written is read by some brief; none is boilerplate.
- Every requirement traces to something the user said or the project
  has; every placeholder is marked.
- Names match across documents: the same entity, field, route and file in
  ERD, API, PLAN and the briefs.
- `PLAN.md`: no file owned by two parts of one wave; every part has an
  agent, files, the documents it reads, and a done-when.

### Avoid in the plan

Plans for small tasks; documents nobody reads; invented requirements,
personas or metrics; sections filled with "N/A"; layouts and code in the
plan; briefs that paste documents instead of naming them; a contract that
differs between briefs; documents left stale after the work changed them.
