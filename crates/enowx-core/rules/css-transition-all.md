---
name: css-transition-all
description: `transition: all` animates every property that changes
severity: remind
match: transition\s*:\s*all\b
match: transition-property\s*:\s*all\b
files: *.css, *.scss, *.sass, *.less, *.tsx, *.jsx, *.vue, *.svelte
---

`all` animates layout properties (width, height, padding) that make the
browser lay the page out every frame, and colours you did not mean to
animate. Name what moves: `transition: opacity 160ms ease-out, transform
160ms ease-out;` (the `motion-performance` skill).
