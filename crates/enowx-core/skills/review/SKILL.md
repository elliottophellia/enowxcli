---
name: review
description: "Reviewing a change for defects: understanding what it claims to do, reading the whole diff then each file in context, the questions to ask of any change (correctness, edge cases, errors, concurrency, security, performance, tests, compatibility), running the tests when it settles something, ranking findings by consequence, what every finding must say, and saying plainly when nothing is wrong. Read before any code review."
---

# Reviewing a change

The generated review opens with style nits, rewrites the author's code in
its comments, asks to "consider adding tests" without naming a case, raises
concerns nobody could reproduce to look thorough, misses the missing
authorisation check, and approves because the tests pass. A useful review
knows what the change claims, reads it in context, settles doubts by running
something, and reports each defect as what breaks, for which input, at which
line, ranked by consequence. When nothing is wrong, it says so. The defects
each kind of change tends to hide are listed in `review-checklists`.

## 0. Stance: firm, tidy, honest

- **Say what it is.** Bad work is called bad, good work good, each with the
  reason. No softening ("you might consider", "perhaps"), no compliment
  before the bad news, no praise to balance a list of defects. A finding
  is stated as a fact when it is verified and marked suspected when not.
- **One verdict, first line, from this scale:**
  - `Good`: no blocker or major, tidy; ship it.
  - `Good with fixes`: no blocker, a few majors or minors with clear fixes.
  - `Needs work`: a blocker, or majors enough that it should not ship.
  - `Bad`: wrong at the root (the approach, the design, the data model);
    fixing lines will not save it. Say what should replace it.
  The verdict follows from the findings, never from effort, size or the
  author's confidence.
- **Tidiness is part of quality.** Inconsistency is a finding, ranked
  Minor (Major when it spreads): names that follow two conventions, the
  same logic written twice, dead code and commented-out blocks, a pattern
  the codebase does one way done another, files in the wrong place, magic
  numbers, a layout whose spacing, edges or card sizes drift. Only pure
  taste is a nit.
- **Not harsh for its own sake.** No concern invented to look thorough, no
  defect raised without the line and the input. When it is good, say so
  plainly and name what makes it good, so it is kept.

## 1. Get the change

- A branch: `git diff --stat main...HEAD` for its shape,
  `git diff main...HEAD` for its content (three dots: only what the branch
  added since it left `main`), `git log --oneline main..HEAD` for its
  commits. Use the base the brief names (`origin/main`, `develop`).
- Work in progress: `git status --short`, `git diff`, `git diff --staged`.
- One commit: `git show <sha>`. A pull request, when `gh` is installed and
  signed in: `gh pr view 123 --json title,body,files` and `gh pr diff 123`.
- `git diff -w` hides whitespace-only changes; `--name-status` lists added,
  deleted and renamed files.
- The old version of a file: `git show main:src/orders/refund.ts`. A
  reviewer's `bash` refuses what changes the tree (`checkout`, `stash`,
  `worktree`, installs, formatters with `--write` or `--fix`), so read the
  base this way instead of switching to it.
- No git, or no diff: review the files the brief names as they stand, and
  say the review covers files, not a change.

## 2. Know what it claims before judging it

- Read the task, the issue, the PR description, the commit messages and
  the brief. Write down in a sentence or two what the change claims to do
  and what it must not break: callers, the public API, stored data,
  config, other screens.
- Read the tests it adds or changes first: they are the author's own
  statement of the behaviour.
- An unclear claim is reviewed against its most plausible reading, and the
  report names that reading.
- What the change does without claiming it (a drive-by refactor, a changed
  default, a new dependency) is reviewed like the rest and named.

## 3. Read twice: the shape, then each file in context

1. **The whole diff once**, quickly: which files, what kind of change
   (feature, fix, refactor, migration, config, dependency), how big. Then
   choose the order: whatever touches money, auth, stored data,
   migrations, the public API or concurrency first; generated files
   (lockfiles, snapshots, generated clients) skimmed for surprises only.
2. **Each file with its surroundings.** Read the whole function, not the
   hunk: an early return, a lock or a transaction boundary outside the
   hunk decides whether it is right. Open the callers
   (`git grep -n 'refundOrder('`), the callees, the config and migrations
   it relies on, and its tests.
3. **The lines that went away.** A deleted `if`, a dropped `await`, a
   removed `AND tenant_id = $2` is a change too. Read every `-` line.
4. **What is missing.** A new enum value that a `switch` elsewhere does not
   handle; a new field absent from the serialiser, the migration, the API
   docs or `.env.example`; a renamed function still called by its old name
   (`git grep -n oldName`); a changed signature with callers not updated.

Past a few hundred changed lines, attention drops. Review in slices by
risk, and say in the report which parts only got a skim.

## 4. The questions to ask of any change

**Does it do what it claims, for every input?** Walk the inputs, not the
happy path:

| Input | What goes wrong |
|---|---|
| Empty | `""`, `[]`, `0`, `null`, a missing key, no rows: `items[0]` on nothing, a division by zero, an empty state never shown |
| One, many | off by one at the ends of a range (`<` against `<=`), the last page, ties in the sort order |
| Large | 50,000 rows loaded at once, a 10 MB body, a regex run on unbounded text |
| Invalid | the wrong type, negative, `NaN`, a date in the past, an id that belongs to someone else |
| Text | Unicode (`'\u{1F44D}'.length === 2` in JavaScript), combining marks, right to left, case folding |
| Time | time zones, DST changes, month ends, leap days, a date parsed as UTC here and local there |
| Money | floats, rounding, the currency's minor unit |
| Twice at once | two tabs, a double click, a retry after a timeout |

**Errors and cleanup.** When the call in the middle fails, is the
transaction rolled back, the file or connection closed, the lock released?
Is the error swallowed (`catch {}`, `except: pass`, `_ = err`), logged and
ignored, or shown to the user with internals in it?

**State and concurrency.** Read, decide, then write while another request
does the same (a lost update); double submits; a retry of something that
is not idempotent; events handled out of order; a promise nobody awaits; a
lock held across an `await`; a cache left stale by the write.

**Security.** Untrusted input reaching a sink: SQL, a shell, a file path,
HTML, a redirect, a URL the server fetches, a deserialiser. A route without
authentication; a record loaded by id without checking that it belongs to
the caller or the tenant; a secret in code, a log or a client bundle;
personal data in logs. `security-web` and `security-auth` hold the classes.

**Performance.** A query inside a loop, a list without a limit, a whole
table in memory, a response that grows with the data, work repeated per
render or per request, a new filter without an index, a heavy import on a
page. Say the size at which it hurts.

**Compatibility.** A field renamed or removed, a status code or meaning
changed; a migration that locks a big table or cannot be undone; whether
the previous version still runs against the new schema during the deploy,
and whether the change can be rolled back; a new required environment
variable without a default; data already stored, queued or cached in the
old shape; mobile clients that cannot be forced to update; a feature flag's
default.

**Tests.** Would the new test fail without the change? Read its assertion
against the old code (`git show main:<path>`). Does it cover the case the
change is about? Is it deterministic (clock, randomness, order, network)?
Does it assert an outcome, or only that a mock was called? Were tests
deleted, skipped (`.skip`, `xit`, `@Ignore`, `#[ignore]`, `t.Skip`) or
their expected values edited until they passed?

**Readability,** only where it will cost someone: a name that says the
wrong thing, a second copy of a helper that already exists (grep for it),
a pattern unlike the rest of the codebase for no reason, a comment that is
no longer true. Formatting is the formatter's job.

## 5. Run something when it settles a question

A defect shown by a failing test or a printed value outranks one argued
from reading. Run the narrowest thing that decides:

```sh
npx vitest run src/orders            # or: pytest tests/test_orders.py -k refund
go test ./orders/... -run TestRefund -race
cargo test orders:: && cargo clippy -- -D warnings
npx tsc --noEmit                     # types across the whole project
python3 -c 'print(0.1 + 0.2)'        # 0.30000000000000004: why money is not a float
TZ=America/New_York node -e 'console.log(new Date(2026, 2, 8, 2, 30).toString())'
```

The last line shows what a local time that does not exist (the DST change
on 8 March 2026) turns into.

- Interfaces: `ui_check` on the changed files, and `preview` at 360, 768
  and 1440px, with `login` and a test account for screens behind a
  sign-in and `motion: true` when something animates. Judge what they
  report with `ui-audit` and `motion-audit`, and against `DESIGN.md`.
- Passing tests are not approval: they prove what they assert. Check that
  they run the changed lines at all.
- Report what you ran and what it printed. What you could not run says
  why ("not run: dependencies not installed"), and its finding stays
  marked as read, not reproduced.
- Leave nothing running; `preview` starts and stops a dev server itself.

## 6. Rank by consequence

| Severity | Means | For example |
|---|---|---|
| Blocker | Must not ship: data lost or corrupted, a security hole, the main path broken, a crash, a migration that cannot be undone safely | `GET /invoices/:id` returns any customer's invoice; a migration drops a column the running code still reads |
| Major | Wrong in a plausible case, an error path that will happen, new logic without a test | the last page repeats rows when two share a timestamp; a failed payment leaves the order `pending` for good |
| Minor | An unlikely edge case, or maintainability that will cost soon | a leap day shown as 1 March; a second copy of an existing helper |
| Nit | Optional; the author may ignore it | a clearer name |

- Consequence times likelihood; not ease of fix, not the order found.
- Never raise a nit to look useful. Three nits at most, or none. A real
  inconsistency is not a nit: it is Minor (section 0).
- Doubt does not raise severity: mark the finding suspected and say what
  would confirm it.

## 7. What every finding says

```
[Major] A refund can be paid out twice on a double click
Where: src/orders/refund.ts:42-58 (refundOrder)
Breaks: two POST /orders/:id/refunds in the same second both pass the
  status check on line 47, and both call the payment provider.
Why: the order is read (line 47), then updated (line 55) with no
  condition, lock or idempotency key.
Fix: update conditionally (WHERE id = $1 AND status = 'paid') and go on
  only if one row changed; send an idempotency key to the provider.
Evidence: read; not reproduced (no test database).
```

- The title is the consequence ("can be paid out twice"), not the
  mechanism ("race condition in refund.ts").
- Where: the path and line or range, and the function.
- Breaks: the input or sequence that triggers it, concretely.
- Why: the lines, the output or the test that shows it.
- Fix: a sentence or a few lines giving the direction, not the author's
  function rewritten.
- One cause, one finding: the same missing check in four handlers is one
  finding with four locations.

## 8. The report

```
Needs work: 1 blocker, 2 major, 1 minor.
Covered: git diff main...HEAD, 14 files (+420 -96); package-lock.json skimmed.
Ran: npx vitest run src/orders (38 passed); npx tsc --noEmit (clean);
  ui_check src/app/checkout (1 medium, finding 4).
1. [Blocker] ...
Not covered: the end-to-end suite (needs the database).
```

- The verdict first (`Good`, `Good with fixes`, `Needs work` or `Bad`,
  section 0) with the counts, then what was covered and run, then the
  findings by severity, then what was not covered.
- Nothing wrong: "Good. No defects found in <scope>. Ran <commands>. Not
  covered: <what>." Never invent a concern to fill the space.
- Praise only what the author should keep ("the migration is additive and
  safe to run before the deploy").
- A question is a finding with a question mark: "Is `limit` meant to be
  unbounded? At 50,000 rows this loads them all."
- About the code, never the person; no "just", "simply", "obviously".
- **Delegated:** the caller receives only the text from `DONE:` on, and
  anything written before it is dropped. So the review goes in the report:
  `DONE:` with the verdict on its first line and the findings indented
  under it, `CHANGED: none`, `VERIFIED:` what you ran and what it printed,
  `NEXT:` the blockers to route first, or `nothing`.
- Handed the conversation instead, answer the user in the shape above.

## 9. Interface and server work

- Interface: the marks of generated work are defects too: invented
  figures, dead controls, default gradients, identical card grids,
  buzzword copy, layouts that break on a phone (`ui-audit`). Every state
  (empty, loading, error), keyboard and screen reader, both themes when
  there are two. Run `preview` on every screen: each line under
  `layout` is a finding (sections on different left edges, overlaps,
  content still hidden and an empty page below are major; spacing, type
  scale, line length and uneven rows are minor unless they are
  everywhere). Visual consistency is reviewed as seriously as code.
- Server: for each endpoint, validated input, an ownership check, one
  error format with nothing internal leaked, transactions and race-free
  writes, paginated lists, no query in a loop, no secret in code or logs
  (`backend` and its part skills).

## Check it

- Each blocker and major: can you point at the line and name the input?
  If not, mark it suspected or drop it.
- Every `-` line read; callers of changed signatures found with `git grep`.
- New tests: would each fail on the base version?
- What you ran is in the report with its result; what you did not run
  says why.
- `ui_check` and `preview` output read, not only run.
- The verdict matches the findings: no approval beside a blocker, no
  `Bad` without saying what should replace it, and no softened word where
  the finding is verified.

## Avoid

Style nits first; the author's code rewritten in comments; "consider
adding tests" without naming the case; concerns you cannot tie to a line
and an input; approving because the tests pass; reading only the hunks;
severity by ease of fix; a guess reported as fact; quoting a secret you
found (name the file and line, never the value); editing files; leaving a
server running.
