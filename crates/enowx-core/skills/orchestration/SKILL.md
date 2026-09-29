---
name: orchestration
description: "Running a large task across specialists: deciding answer, handoff or delegation, splitting by area into parts, waves (foundation, build, check), the contract shared by parts that run together, file ownership, briefs that carry the user's decisions and nothing invented, tiers, tracking progress with todo, handling a part that fails, and reporting back. Read before delegating work that touches several areas."
---

# Running a large task across specialists

Left to habit, an orchestrator reads its way through the project before
routing a request that already named its kind of work, delegates one part,
waits, delegates the next; sends everything to `general` on `strong`;
writes briefs that prescribe a layout and drop what the user decided; lets
two agents edit one file; and passes "done" on without looking. This is how
to choose between answering, handing off and delegating, split by area, run
waves on a shared contract, brief each part, choose tiers, handle failures
and report back. Agreeing an open design with the user is `brainstorm`.

## 1. Answer, hand off or delegate

| The request | Do |
|---|---|
| Conversation, or a question a few looks settle (where X is defined, what changed, which version, do the tests pass) | Answer: at most five reads, searches or commands, then paths and lines or the output |
| A question that needs more looking | `delegate` to `research`; to `librarian` for raw excerpts you need to plan a split |
| One specialist's work: a fix, a small change, one page, a follow-up on its own work | `handoff`: it works in this conversation, answers the user, and the conversation comes back after its reply |
| Several areas or pages, a new project, a redesign, more than a handful of files, or the user asks for speed or sub-agents | Split into parts, `delegate` them in waves |
| A one-off result you will report: a review, an audit, an investigation | `delegate` |

- Size: one specialist and under an hour of work is light; several areas,
  several pages or a new project is large. Asked for speed, split wider:
  more parts, each smaller.
- Choose by the work, not the words: "the login page is broken" is `fe`
  when it renders wrong and `be` when the request fails. A domain agent
  when the work sits in one part of the stack ("review the API changes" is
  `be`); a cross-cutting one (`review`, `test`, `security`, `perf`,
  `docs`) when it spans several. Animation is `motion`'s; the hover and
  open states of a component come with the `fe` work that builds it.
  `general` only when nothing fits, and then say the roster lacks an agent.
- A request that names its kind of work is routed without reading first:
  the specialist reads what it needs.

## 2. Split by area

- A part is one area and its files, owned by one specialist for one run:
  each page or screen its own `fe` part, the API a `be` part (or one per
  service), the schema and migrations a `db` part, each app screen a
  `mobile` part, animation a `motion` part once the markup exists.
- Never by activity on the same files: "build" and "test" of one module, or
  "markup" and "styles" of one page, cannot run side by side.
- Sized to finish in one run: a page, a group of endpoints, a set of
  migrations. Bigger: split by page or by resource. The same specialist
  may take several parts at once; four pages are four `fe` delegations.
- A bug with an unknown cause is not split. Find the cause first (`test` to
  reproduce it, or `research`), then give the fix to the owner of the files.

## 3. Waves

A wave is one step holding one `delegate` call per part, so its parts run
at the same time; it waits only for the wave before it.

0. **Plan**, for a new product or a feature across several areas or waves:
   the documents the user chose in `brainstorm` (PRD, DESIGN, ARCHITECTURE,
   ERD, API, PLAN), written with `plan_write` from what the user decided;
   none when they chose none. The waves below are `PLAN.md`'s; the contract is `API.md`'s.

1. **Foundation**, only when the other parts stand on it: a new project's
   scaffold, shared types, the API's routes and shapes, the layout every
   page sits in, the tokens and `DESIGN.md`. For an interface built in
   several parts, the foundation writes the layout system they share
   (`ui-layout`, "The measures as code"): the container and its width, the
   section spacing, the spacing and type scales as tokens, the section
   heading pattern. Parts built side by side without it each invent their
   own width and spacing, and the page comes out misaligned. One part, no bigger than
   that, by the specialist of the main framework (`fe` for a Next.js app,
   `be` for an API service). Skipped when the project exists.
2. **Build**, as wide as the work allows: every independent part in one
   step. `motion` needs markup that exists: on existing pages it runs in
   this step on its own files; on new pages it takes a step of its own,
   right after the `fe` parts it animates.
3. **Check**: the checks the user chose, together: `test`, `review`,
   `security` where it matters (sign-in, payments, uploads, anything
   public). They are asked in `brainstorm`, or in one `ask` before the
   first wave when there was no brainstorm. A check not chosen is not
   delegated; none chosen means no check wave, and the answer says so.
4. **Fixes** from the check, each to the specialist that owns the files,
   with the finding as its brief.

A wave runs in the background. Once its `delegate` calls are made, end
the turn with one line to the user saying who is working on what; do not
wait, poll or read their files. When every part of the wave has finished,
their reports start your next turn together, and you go on from them. The
user can talk to you in between; answer, and leave the running parts
alone.

Never delegate an independent part, wait for it, then delegate the next:
that is parallel work run in series. Never split one part into steps
delegated one after another: the specialist plans its own steps.

## 4. The contract between parts at work together

- **Files.** Each brief names the files or folders its part owns. A file
  every part needs (the router, the package manifest, the shared
  stylesheet, the root layout, a shared types module) belongs to the
  foundation or to one part alone. enx closes a file one agent is editing
  to the others until it finishes; an agent refused a file goes on with its
  other files and reports what it needed, so shared ownership turns into
  waiting or half-done work.
- **Interfaces.** Parts that meet get the same text in their briefs:
  routes and methods, request and response shapes, status and error
  codes, the names and home of shared types, event names, environment
  variable names, each page's URL.
- **Coordination, not design:** routes and shapes, never the layout or how
  a part is built.

```
Contract (the same text in the be brief and every fe brief):
- GET  /api/products?cursor=&limit=        200 { items: Product[], nextCursor: string | null }
- GET  /api/products/:slug                 200 Product, or 404 { code: "not_found" }
- POST /api/cart/items { productId, qty }  201 Cart, or 409 { code: "out_of_stock" }
- Product { id, slug, name, priceMinor, currency, imageUrl } and Cart live in
  src/shared/types.ts, written by the foundation; nobody edits it in this wave.
- Money in integer minor units with a currency code.
Files: be owns src/server/**; each fe part owns src/app/<its page>/**.
```

## 5. The brief

A delegated specialist sees the brief and nothing else:

```
Goal: <what the user wants, in their words, one to three lines>
Read: <the plan documents and sections, when there is a plan:
  docs/plan/PRD.md (FR-1 to FR-3), docs/plan/API.md (Products), DESIGN.md>
Decided: <every decision the user made: audience, scope, concept, theme,
  stack, where content comes from; placeholders named as placeholders>
Your part: <scope>, in <the files or folders you own>
Beside you: <the other parts running now, and what they own>
Contract: <routes, shapes, shared types, names: the same text in each brief>
Constraints: <what the user stated: libraries to use or avoid, what must not break;
  for a section of a page: "use the foundation's .container, --section and --text-*;
  set no content width, outer margin or font size of your own">
Out of scope: <what the user deferred; files that are not yours>
Done when: <it builds, its tests pass, preview is clean at 360, 768 and 1440px,
  its layout lines included>
```

- The user's decisions travel whole, in their own words where they gave
  any.
- Nothing invented: no requirement the user did not state, no sections or
  contents ("a hero with stats"), no step-by-step plan. The specialist's
  skills decide the layout and the steps.
- File names, never file contents: the specialist reads what it needs.
- A part whose product is text (a review, an answer, excerpts): say it goes
  in the report under `DONE:`. A report reaches you from `DONE:` on, and
  whatever the specialist wrote before it is dropped.
- A handoff needs no brief, since the specialist has the conversation. Its
  `reason` is shown to the user: one line on why.

## 6. Tiers

| Tier | For | For example |
|---|---|---|
| `cheap` | Mechanical work against a clear specification | a rename across files, a stated pattern applied, files gathered |
| `balanced` | Normal implementation, known shape or known cause | a page from an agreed design, CRUD endpoints, tests for existing code |
| `strong` | Judgement: unclear cause, design decisions, unfamiliar code, costly mistakes | an intermittent production bug, a migration on a live table, an architecture choice |

- Start one tier lower than feels right: a `balanced` attempt that fails
  costs less than a `strong` run nobody needed.
- Leave `tier` out to use the agent's own: `review`, `security`, `perf`
  and `systems` run `strong`, `librarian` `cheap`, the rest `balanced`.
- After a failure of judgement (it misread the problem, went in circles),
  re-delegate one tier up with the problem named. A failure from a missing
  fact is fixed in the brief, not with the tier.
- When a model cannot be reached, enx falls back a tier and says so in the
  transcript; weigh that part's report accordingly.

## 7. Track it with `todo`

For four parts or more: one item per part, grouped by wave, set once before
the first delegation and marked done together as each wave returns.

```
[1] fe: scaffold, root layout, tokens, DESIGN.md, shared types
[2] fe: catalogue page
[2] fe: product page
[2] fe: cart and checkout
[2] be: products, cart and orders API
[2] db: schema and seed data
[3] test: API and checkout flow
[3] review: all changes
```

## 8. When a part fails or reports a problem

Read the whole report: `DONE`, `CHANGED`, `VERIFIED`, `NEXT`. One opening
with `NO REPORT` ended without a report; its `CHANGED` line is taken from
the tool calls, not from what the agent said.

| What came back | Do |
|---|---|
| Failed before changing anything (wrong agent, declined, no model) | Route it again, or re-delegate with a better brief |
| `PARTIAL FAILURE`, `NO REPORT`, or out of steps | Continue it: `delegate` to the same specialist with `resume` set to the session id its report ends with, and a task saying what is left (below). It keeps everything it read and changed, so nothing is redone. A second failure goes to the user with the files left half-done |
| It needed a file another part owns | Give that change to the file's owner, or the file to one part in the next wave |
| A contract mismatch (`/api/product` against `/api/products`) | Fix the contract in both briefs and route the change to one side |
| A question only the user can answer | `ask` the user, with options, then re-delegate |
| It went in circles or misread the problem | Continue it with `resume`, naming the specific issue, one tier up; a fresh delegation only when its context is the problem |
| A follow-up on a part that finished (a finding, a change the user asked for) | `resume` its session too: it already knows those files |
| Findings from the check wave | Blockers and majors to the owners of the files, finding as brief; minors listed for the user |

```
delegate(agent: "fe", resume: "<session id from its report>",
  task: "You ran out of steps. The cart page is written; what is left is the
  quantity stepper and the empty state (FR-6). Finish those, check, and report.")
```

Never absorb a part's work yourself: you have no tool that edits code, and
the shell refuses commands that change files. `plan_write` writes the
planning documents and nothing else.

## 9. Check the reports

- A report is a claim, and `VERIFIED` is the line that matters: pass
  `not verified` on to the user as exactly that.
- When the user will rely on "the tests pass" or "it builds", run that
  command once (`npm test`, `pytest -q`, `cargo test`), and
  `git diff --stat` for what changed. That is a check, not a second review.
- Do not re-read a specialist's files unless its report leaves something
  the user asked about unclear.

## 10. Splits that work

- **A new shop.** Brainstorm; then 1) `fe` foundation; 2) `fe` catalogue,
  `fe` product page, `fe` cart and checkout, `be` API, `db` schema and
  seed, all in one step; 3) `motion`, if the user chose motion; 4) `test`,
  `review`, and `security` on checkout and sign-in.
- **A dashboard in an existing SaaS app.** No foundation: 1) `fe` per
  screen (overview, list and detail, settings), `be` for the new
  endpoints, `db` for the migrations and indexes the new queries need;
  2) `test` and `review`.
- **A mobile app on an existing API.** 1) `mobile` scaffold with navigation
  and the API client, when there is no app yet; 2) `mobile` per screen,
  `be` for the missing endpoints; 3) `test`, `review`, and `security` for
  token storage and sign-in.
- **A landing page with motion.** Brainstorm the look, the theme and how
  much motion; then 1) one `fe` part for the page (its sections share one
  stylesheet and layout, so they are not split across agents); 2) `motion`
  on its own files; 3) `review`, which looks with `preview`.
- **"Checkout sometimes charges twice."** Not split yet: 1) `test` to
  reproduce it (`strong` while the cause is unclear); 2) the fix to its
  owner, or to both at once when files do not overlap (`be`: an idempotency
  key; `fe`: no double submit); 3) `review`.

## 11. Report back

At most six one-line bullets:

```
- Built: <each part in a line: what, and by which specialist>
- Checked: <commands run and their results; the review's verdict, what was fixed>
- Placeholders: <content marked as sample, and where it lives>
- Left for you: <decisions, keys, anything not verified>
```

Say which specialist did what, and never present its work as your own.
Say plainly what failed, what was not verified and what is left.

## Check it

- Before the first delegation: parts in `todo` by wave; no file owned by
  two parts of a wave; every brief with goal, decisions, owned files, the
  parts beside it, contract and done-when; independent parts in one step.
- After each wave: every report read; `NO REPORT` and partial failures
  handled as in section 8.
- Before answering: tests or build run once if the user relies on them,
  `git diff --stat` read; the answer says built, verified and left.

## Avoid

Doing the work yourself; reading the project before routing a request that
named its kind; independent parts delegated one after another; splits by
activity; two parts owning one file; briefs with invented requirements,
prescribed sections or pasted files; `general` and `strong` by default;
retrying a partial failure more than once; reports passed on unread;
questions to the user as prose instead of `ask`; a specialist's work
presented as your own.
