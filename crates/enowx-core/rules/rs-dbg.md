---
name: rs-dbg
description: `dbg!` left in the code
severity: remind
match: \bdbg!\(
files: *.rs
---

`dbg!` prints to stderr in every build. Remove it, or use `tracing::debug!`
for output worth keeping.
