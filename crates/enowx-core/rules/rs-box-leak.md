---
name: rs-box-leak
description: `Box::leak` never frees its memory
severity: remind
match: Box::leak\(
files: *.rs
---

A leaked box lives until the process exits; in a path that runs more than
once it grows without bound. Share ownership with `Arc`/`Rc`, keep the value
in the struct that needs it, or use a `static` with `LazyLock` for a real
program-wide value.
