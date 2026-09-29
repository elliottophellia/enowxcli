---
name: js-eval
description: `eval` or `new Function` runs text as code
severity: remind
match: \beval\(
match: \bnew\s+Function\(
files: *.ts, *.tsx, *.js, *.jsx, *.mjs, *.cjs
---

Code built from strings is an injection point and defeats bundlers and
content security policies. Parse data with `JSON.parse`, look behaviour up in
a map of functions, or use a real parser for the expression language.
