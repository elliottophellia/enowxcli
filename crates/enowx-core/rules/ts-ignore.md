---
name: ts-ignore
description: `@ts-ignore` hides the error and any later one on that line
severity: remind
match: @ts-ignore
files: *.ts, *.tsx
---

Fix the type instead. When the error is real but cannot be fixed here (a
wrong upstream type), use `// @ts-expect-error <why>`: it says why, and it
fails the build once the error is gone, so it does not outlive its reason.
