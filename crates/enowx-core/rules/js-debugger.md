---
name: js-debugger
description: A `debugger` statement left in the code
severity: remind
match: ^\s*debugger;?\s*$
files: *.ts, *.tsx, *.js, *.jsx
---

Remove it: it stops every visitor's page that has the devtools open.
