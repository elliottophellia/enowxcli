---
name: js-empty-catch
description: An empty `catch` swallows the error
severity: remind
match: catch\s*(\([^)]*\))?\s*\{\s*\}
files: *.ts, *.tsx, *.js, *.jsx, *.mjs, *.cjs
---

An error caught and dropped is a failure nobody hears about. Handle it
(show the user what went wrong, retry, fall back) or let it propagate. When
ignoring it really is right, say why in the block:
`catch { /* storage full is fine: the draft is also in memory */ }`.
