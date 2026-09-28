---
name: ui-stack-shadcn
description: "Building with shadcn/ui without the shadcn-default look: using and adding its components, theming its CSS variables, extending variants. Read when the project has components/ui from shadcn."
---

# shadcn/ui

The components are copied into the project (`components/ui/`), built on
Radix primitives, Tailwind and `cva`. Untouched, they give the look everyone
recognises: neutral grey, the default radius, cards in a grid.

## Use what is there

- Before writing a part, check `components/ui/`: Button, Card, Dialog,
  Sheet, DropdownMenu, Popover, Tooltip, Tabs, Table, Form, Input, Select,
  Checkbox, Switch, Badge, Skeleton, and Sonner for toasts.
- Add a missing one with `npx shadcn@latest add <name>` rather than writing
  it by hand, then adjust it to the project.
- One component library: do not bring in another alongside it.

## Make it the product's

- The theme is the CSS variables in `globals.css` (`--background`,
  `--foreground`, `--primary`, `--muted`, `--accent`, `--border`, `--ring`,
  `--radius`, and the dark set). Set them from DESIGN.md: the palette, the
  radius, the fonts. Leaving the defaults is leaving the direction unset.
- A new variant goes into the component's `cva` definition (`variant`,
  `size`), used by name, not a pile of classes overriding it at each use.
- Compose parts from the primitives instead of forking them.

## Forms

`Form` with react-hook-form and a zod schema when the project uses them:
field-level messages, the submit button disabled while submitting.

## The default look to avoid

A dashboard of identical Cards with a number and a green delta, the
untouched zinc theme, a Badge on everything, Dialogs for what should be a
page.
