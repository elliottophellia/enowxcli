---
name: ui-stack-svelte
description: "Building interfaces in Svelte 5 and SvelteKit: runes, props, snippets, keyed each blocks, transitions, load functions. Read when the project uses Svelte or SvelteKit."
---

# Svelte 5 and SvelteKit

Match the project's Svelte version: these are Svelte 5's runes; a Svelte 4
project uses `export let` props and `$:` statements instead.

## Components

- One component per `.svelte` file. Props with `let { title, variant =
  'primary', ...rest } = $props()`, spread `rest` onto the native element a
  primitive wraps.
- Content through snippets (`{@render children()}`) rather than a prop per
  variation.
- Styles in the component's `<style>` (scoped by default) or the project's
  Tailwind; tokens as CSS variables in one global place.

## State

- `$state` for what changes, `$derived` for what follows from it; `$effect`
  only for work outside Svelte (a subscription, the canvas), with cleanup.
- `{#each items as item (item.id)}`: keyed by a stable id.
- Loading, error and empty as branches (`{#await}` or explicit states).

## Motion

`svelte/transition` for purposeful motion, disabled under
`prefers-reduced-motion`.

## SvelteKit

Data in `+page.server.ts` or `+page.ts` load functions, `+error.svelte` for
failures, `+layout.svelte` for the shared frame, `<svelte:head>` for title
and description, form actions for forms. Check with `npm run build` and
`npm run check`.
