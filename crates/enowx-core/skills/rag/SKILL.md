---
name: rag
description: "Searching this project's code by meaning with the built-in rag server: when a search beats reading files, how the index keeps itself fresh, writing a query, reading the results (whole functions, with where they sit and what they define), and when to fall back to grep. Read before exploring a large or unfamiliar codebase."
---

# Searching the code with rag

The rag server keeps an index of this project in Postgres with pgvector,
embedded by the model the user chose in Settings > RAG. Files are cut along
their syntax: a function, a type, a class method or a config key is one
chunk, with the comments above it, and never cut in half; a chunk says where
it sits (`in impl Store`, `in Install > macOS`) and what it defines. A search
finds the chunks that answer a question by meaning and by words, and reranks
them, so one call often replaces a dozen reads and greps.
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

## 2. The index keeps itself fresh

- The workspace is indexed when the session starts. A file you or another
  agent write or edit is indexed again the moment the tool returns; other
  changes (the user's, a `git pull`) are picked up every half minute, and
  everything is checked again right before each search. You do not need to
  call `index` before searching or after editing.
- Only new or changed code is embedded; code that only moved keeps its
  embedding. Removed code leaves the index.
- Call `index` yourself only for another folder (`path`), when a search
  says indexing is still running and you need the newest code now, or when
  `status` shows `auto_index` off.
- A search on a project not indexed yet says so: call `index`, then search
  again.
- `.gitignore` is honoured, and `.env` files, lockfiles and minified
  bundles are never read. Code that is ignored is not in the index: say so
  if that is what the user asked about.

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

- Each result is `path:start-end`, where it sits and what it defines, and
  its score, best first: `## src/cart.rs:40-88 · in impl Cart · total,
  apply_coupon (score 0.71)`. A chunk holds whole items, so a function in a
  result is the whole function. Read the file at that range with `read`
  before you change it: the lines around it (imports, the type it belongs
  to, its callers) are not in the chunk.
- Scores rank, they do not prove. A result that does not answer the
  question is not the answer because it came first: search again with a
  sharper query, or fall back to `grep`.
- Cite what you found by `path:line` in your answer or report.

## 5. When it fails

- "not indexed yet": call `index`, then search again.
- A database or embedding error: say so plainly and fall back to `glob`,
  `grep` and `read`. The work does not stop because the index is down.
- Never print the database connection string or the API key; they are
  configured by the user, not by you.
