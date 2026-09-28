---
name: ui-stack-vue
description: "Building interfaces in Vue 3 and Nuxt: single-file components, script setup, props and emits, composables, lists, transitions, Nuxt built-ins. Read when the project uses Vue or Nuxt."
---

# Vue 3 and Nuxt

## Components

- Single-file components with `<script setup lang="ts">`, named for what
  they are, one per file.
- Props with `defineProps<{ … }>()`, events with `defineEmits`, `v-model` on
  components through `defineModel`. Slots for content, not a prop per
  variation.
- Logic that several components share goes in a composable (`useX`), not a
  mixin.
- Styles scoped, or the project's Tailwind; tokens as CSS variables in one
  place.

## State and lists

- `ref` and `computed`; derive with `computed` instead of watching one value
  to set another. `watch` for effects outside the component.
- `v-for` with a stable `:key` from the data; never `v-if` and `v-for` on
  the same element.
- Loading, error and empty as three branches in the template.

## Motion

`<Transition>` and `<TransitionGroup>` for purposeful motion, turned off
under `prefers-reduced-motion`.

## Nuxt

`NuxtLink` for internal links, `NuxtImg` when `@nuxt/image` is installed,
`useHead` or `useSeoMeta` for titles and descriptions, `useFetch` or
`useAsyncData` for data with its pending and error states, layouts in
`layouts/`. Check with `npm run build`.
