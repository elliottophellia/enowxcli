---
name: ts-no-any
description: `any` switches the type checker off
severity: remind
match: :\s*any\b
match: \bas\s+any\b
match: <any>
files: *.ts, *.tsx
exclude: *.d.ts
---

`any` turns off checking for the value and everything it touches, so the
error it hides shows up at run time instead.

- Unknown input (JSON, an event, a catch variable): `unknown`, narrowed with
  a check or a schema (`zod`, `valibot`) before use.
- A value of several shapes: a union, discriminated by a field.
- A generic helper: a type parameter (`<T>`).
- A third-party gap: a small local type for the part you use.
