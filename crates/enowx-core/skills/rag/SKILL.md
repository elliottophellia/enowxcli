---
name: rag
description: "Searching this project's code by meaning with the built-in rag server: when a search beats reading files, indexing before the first search and after big changes, writing a query, reading the results by file and line, and when to fall back to grep. Read before exploring a large or unfamiliar codebase."
---

# Searching the code with rag

The rag server keeps an index of this project: every source file cut into
chunks of whole lines, embedded with Voyage AI, stored in Postgres with
pgvector. A search finds the chunks that answer a question by meaning and by
words, and reranks them, so one call often replaces a dozen reads and greps.
Its tools are `mcp__rag__index`, `mcp__rag__search`, `mcp__rag__status` and
`mcp__rag__forget`.

## 1. When to search, and when not to

- Search when you do not know where something lives: "where are refunds
  issued", "how is the session token checked", "what sends the invoice
  email". In a project of more than a few dozen files this is the first
  move, before any `glob` or `grep`.
- Search for behaviour and intent, not names. To find every use of a known
  symbol, `grep` it or ask `lsp` for its references: exact and complete,
  where a search is ranked and capped.
- A small project you can see whole in a `glob`: read it instead.

## 2. Index first, then keep it fresh

- Call `status` once at the start. No chunks, or a last index older than
  the work in front of you: call `index`.
- `index` embeds only chunks that are new or changed and drops chunks of
  code that is gone, so running it again is cheap. Run it after a large
  change (a refactor, a merge, a generated folder) before relying on a
  search.
- It indexes the workspace by default; pass `path` for another folder.
- It honours `.gitignore` and never reads `.env` files, lockfiles or
  minified bundles. Code that is ignored is not in the index: say so if
  that is what the user asked about.

## 3. Writing the query

- One question in plain words, the way you would ask a colleague: "where
  is the user's plan checked before an export". Not a list of keywords.
- Name the thing and the action. "rate limit on login attempts" beats
  "security".
- Several questions, several searches. A query that mixes two topics
  returns half of each.
- `limit` defaults to 8. Ask for more (up to 30) only to survey a theme
  across the codebase.

## 4. Reading the results

- Each result is `path:start-end` with its score, best first. Read the top
  results, then open the file at that range with `read` for the
  surrounding code before you change anything: a chunk is a window, not the
  whole function.
- Scores rank, they do not prove. A result that does not answer the
  question is not the answer because it came first: search again with a
  sharper query, or fall back to `grep`.
- Cite what you found by `path:line` in your answer or report.

## 5. When it fails

- "not indexed yet": call `index`, then search again.
- A database or Voyage error: say so plainly and fall back to `glob`,
  `grep` and `read`. The work does not stop because the index is down.
- Never print the database connection string or the API key; they are
  configured by the user, not by you.
