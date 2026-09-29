---
name: css-outline-none
description: The focus outline is removed
severity: remind
match: outline\s*:\s*(none|0)\s*[;}]
files: *.css, *.scss, *.sass, *.less, *.tsx, *.jsx, *.vue, *.svelte
---

Without an outline, keyboard users cannot see where they are. Remove it only
together with a visible replacement on `:focus-visible`:
`button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }`.
