---
name: ui-stack-react
description: "Building interfaces in React: component shape, props and variants, state and effects, lists, forms, accessibility, file layout. Read when the project uses React."
---

# React

## Components

- Function components, one per file named for it, props typed (a TypeScript
  `type` or `interface`); match the project's export style.
- Composition over configuration: `children` and named slots rather than a
  boolean per variation. Variants as a `variant` and a `size` prop.
- A primitive (Button, Input) passes native props and the `ref` through
  (`...rest`, and `forwardRef` before React 19, the `ref` prop after), so
  it stays a real button to forms and screen readers.
- Never define a component inside another's render: it remounts every
  time.
- Primitives in `components/ui/`, composed parts in `components/<feature>/`,
  or the project's layout.

## State and data

- State in the lowest component that needs it; lift it when two need it;
  context for what many parts read (theme, the signed-in user), not for
  everything.
- Derive values during render instead of copying them into state and
  syncing with an effect.
- Server data through the project's library (React Query, SWR) or the
  framework's loaders; never a hand-written fetch in an effect when one of
  those is there. Loading, error and empty are three branches in the
  markup, each saying what to do.
- Effects only to synchronise with something outside React (a
  subscription, the document title), with a cleanup.

## Lists and forms

- Keys are stable ids from the data, never the index for a list that
  changes.
- Labels tied to fields with `htmlFor` and an id from `useId`; the error in
  words under the field with `aria-describedby`; the submit button disabled
  and showing progress while pending. Use the project's form library
  (react-hook-form, with zod when it is there).

## Accessibility

Semantic elements, never a clickable `div`. Dialogs, menus, tabs and
tooltips from the project's primitives (Radix, React Aria, shadcn/ui), which
handle focus and keys, rather than hand-rolled.

## Performance

Memoise after measuring, not by habit. Split heavy routes and components
with `lazy`. Images with sizes set.
