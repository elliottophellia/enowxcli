---
name: js-console-log
description: `console.log` left in application code
severity: remind
match: \bconsole\.log\(
files: src/**/*.ts, src/**/*.tsx, src/**/*.js, src/**/*.jsx, app/**/*.ts, app/**/*.tsx
exclude: **/*.test.*, **/*.spec.*, **/scripts/**
---

Debug output left in shipped code leaks data to anyone with the devtools
open and buries real messages. Remove it once the question it answered is
answered; for lasting diagnostics use the project's logger, or
`console.error` for an error the user's report will need.
