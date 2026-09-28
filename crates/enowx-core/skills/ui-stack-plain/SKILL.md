---
name: ui-stack-plain
description: "Building with plain HTML, CSS and a little JavaScript, no framework: files, tokens, component classes, progressive enhancement, themes. Read when the project is, or should be, static HTML and CSS."
---

# Plain HTML, CSS and JavaScript

The right stack for a page of content: nothing to build, nothing to break.
It still needs structure, or it becomes one long file of one-off styles.

## Files

- `index.html` (and one file per page), `styles.css`, and `script.js` only
  when something must behave. Assets in `assets/` or `img/`.
- `<!doctype html>`, `<html lang="…">`, `<meta name="viewport"
  content="width=device-width, initial-scale=1">`, a real `<title>` and
  `<meta name="description">`.
- Landmarks: `header`, `nav`, `main`, `footer`; one `h1`.

## CSS, in this order

1. **Tokens** in `:root`: colours by role (`--paper`, `--ink`, `--muted`,
   `--line`, `--accent`), the spacing scale (`--s-1` to `--s-9`), radii, the
   type scale with `clamp()` for display sizes, font stacks.
2. **Base**: `*, *::before, *::after { box-sizing: border-box; }`, body
   margin 0, `img, svg { max-width: 100%; height: auto; display: block; }`,
   body font and colour from the tokens, a visible `:focus-visible`.
3. **Layout**: `.container` (max-width and side padding), the grid, section
   spacing.
4. **Components**: one class per part and its pieces (`.card`,
   `.card__title`), modifiers for variants (`.btn--primary`). Every value
   from a token.
5. **Media queries**, mobile-first: `@media (min-width: 48em) { … }` next to
   the component they change, not in one block at the end.

- `@media (prefers-reduced-motion: reduce)` turns transitions off.
- A dark theme overrides only the tokens: `[data-theme="dark"] { --paper: …;
  --ink: …; }`, with `prefers-color-scheme` as the default when both are
  offered.

## JavaScript

- `<script src="script.js" defer>` or `type="module"`. The page reads and
  works without it; script adds behaviour (a menu, a copy button, a form
  that submits without reload).
- No globals: a module, or an IIFE. Event delegation on a parent for
  repeated items. `aria-expanded` and `hidden` toggled together with the
  visuals.
- No framework or jQuery for a menu and a form.

## Repetition

The same markup repeated keeps the same class structure, so the CSS styles
it once. Content rendered by script uses a `<template>` element, not HTML
built in strings.

## Fonts and images

- Fonts: `preconnect`, `display=swap`, only the weights used; or a system
  stack.
- Images: `width` and `height`, `loading="lazy"` below the fold, `srcset`
  for photographs, `alt` on every one.

## Check

Open the file in the browser (or the `preview` tool), and run `ui_check`.
