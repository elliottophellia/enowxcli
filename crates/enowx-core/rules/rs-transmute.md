---
name: rs-transmute
description: `mem::transmute` reinterprets bits unchecked
severity: remind
match: mem::transmute
files: *.rs
---

Transmute is undefined behaviour the moment sizes, alignment or validity
differ. Prefer a safe conversion: `from_ne_bytes`/`to_ne_bytes`, `as` for
numbers, `bytemuck::cast` for plain data, `From`/`TryFrom`. If it must stay,
the `unsafe` block says in a `// SAFETY:` comment why it holds.
